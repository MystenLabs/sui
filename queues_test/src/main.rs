use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use futures::stream::{StreamExt, FuturesUnordered};
//use leapfrog::LeapMap;
use once_cell::sync::Lazy;
use rand::{Rng, thread_rng, SeedableRng};
use tokio::sync::{RwLock, Mutex};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

const NUM_OBJ: usize = 10_000;
const NUM_TXS: usize = 100_000;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ObjRef {
    id: usize,
    ver: usize,
}

#[derive(Clone)]
struct Tx {
    inputs: Vec<usize>,
    writing: Vec<usize>,
    time_ms: usize,
}

#[derive(Clone)]
struct TxLocked {
    inputs: Vec<ObjRef>,
    writing: Vec<usize>,
    time_ms: usize,
}

#[derive(Clone, Copy, Debug)]
struct TxResult {
    txid: usize,
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let txs1 = generate_workload();
    let txs2 = txs1.clone();
    let txs3 = txs1.clone();
    let txs4 = txs1.clone();
    let txs5 = txs1.clone();
    let txs6 = txs1.clone();
    let txs7 = txs1.clone();
    println!("Setup: {} ms", start.elapsed().as_millis());

    let start = Instant::now();
    let state_digest_1 = execute_sequential(txs1).await;
    println!("Sequential: {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_2 = execute_dispatcher(txs2).await;
    println!("Dispatcher: {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_3 = execute_dispatcher2(txs3).await;
    println!("Dispatcher 2: {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_4 = execute_queues(txs4).await;
    println!("Queues (watch): {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_5 = execute_queues2(txs5).await;
    println!("Queues (oneshot): {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_6 = execute_queues3(txs6).await;
    println!("Queues (mpsc): {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());

    let start = Instant::now();
    let state_digest_7 = execute_classic_queues(txs7).await;
    println!("Classic Queues: {} ms ({:.0} tps)", start.elapsed().as_millis(), NUM_TXS as f64 / start.elapsed().as_secs_f64());
}

fn generate_workload() -> Vec<Tx> {
    let mut rng = thread_rng();
    (0..NUM_TXS).map(|_| {
        let num_inputs = rng.gen_range(2..=8);
        let inputs: Vec<_> = (0..num_inputs).map(|_| rng.gen_range(0..NUM_OBJ)).collect();
        let writing = inputs.iter().cloned().filter(|_| rng.gen::<f32>() < 0.4).collect();
        Tx {
            inputs,
            writing,
            time_ms: rng.gen_range(5..=25),
        }
    }).collect()
}

async fn execute_sequential(txs: Vec<Tx>) {
    let mut objects = DashMap::new();
    let cur_obj = HashMap::new();
    for (txid, tx) in txs.into_iter().enumerate() {
        let tx_locked = lock_obj(tx, &cur_obj);
        exec_tx_task(txid, tx_locked, &objects, None).await;
    }
}

async fn execute_dispatcher(txs: Vec<Tx>) {
    //static objects: Lazy<RwLock<HashMap<ObjRef, u8>>> = Lazy::new(|| RwLock::new(HashMap::new()));
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::new());
    let mut transactions = HashMap::new();
    let cur_obj = HashMap::new();
    static next_write_tx: Lazy<RwLock<HashMap<usize, usize>>> = Lazy::new(|| RwLock::new(HashMap::new()));
    let mut waiting_on = HashMap::new();
    let mut waited_on_by: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut tx_tasks = JoinSet::new();
    //let mut tx_tasks = FuturesUnordered::new();
    let mut txs_ready = VecDeque::new();

    // Register all transactions with their dependencies.
    let start = Instant::now();
    let mut nwt_guard = (&*next_write_tx).write().await;
    for (txid, tx) in txs.into_iter().enumerate() {
        let mut deps = Vec::new();
        for obj in &tx.inputs {
            if let Some(&dep) = nwt_guard.get(obj) {
                deps.push(dep);
                if let Some(list) = waited_on_by.get_mut(&dep) {
                    list.push(txid);
                }
            }
        }
        for obj in &tx.writing {
            nwt_guard.insert(*obj, txid);
        }
        if deps.is_empty() {
            txs_ready.push_back(txid);
        }
        waiting_on.insert(txid, deps);
        waited_on_by.insert(txid, Vec::new());
        transactions.insert(txid, tx);
    }
    drop(nwt_guard);
    println!("initial registering of dependencies: {} ms", start.elapsed().as_millis());

    loop {
        // Run any transactions which are ready.
        /*while tx_tasks.len() < 12 && !txs_ready.is_empty() {
            let txid = txs_ready.pop_front().unwrap();*/
        for txid in txs_ready.drain(..) {
            let tx = transactions.remove(&txid).unwrap();
            let tx_locked = lock_obj(tx, &cur_obj);

            //
            let objs = &objects;
            let nwt = &*next_write_tx;
            tx_tasks.spawn(exec_tx_task(txid, tx_locked, objs, Some(nwt)));
            //tx_tasks.push(exec_tx_task(txid, tx_locked, objs, Some(nwt)));
        }

        // Join next finished transaction.
        if let Some(j) = tx_tasks.join_next().await {
            let res = j.unwrap();
        //if let Some(res) = tx_tasks.next().await {
            for &tx_waiting in waited_on_by.get(&res.txid).unwrap() {
                if let Some(entry) = waiting_on.get_mut(&tx_waiting) {
                    entry.retain(|&v| v != res.txid);
                    if entry.is_empty() {
                        txs_ready.push_back(tx_waiting);
                        waiting_on.remove(&tx_waiting);
                    }
                }
            }
            waited_on_by.remove(&res.txid);
        } else {
            break;
        }
    }
}

async fn execute_dispatcher2(txs: Vec<Tx>) {
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::new());
    static next_write_tx: Lazy<RwLock<HashMap<usize, usize>>> = Lazy::new(|| RwLock::new(HashMap::with_capacity(NUM_OBJ)));
    let mut transactions = HashMap::with_capacity(NUM_TXS);
    let cur_obj = HashMap::with_capacity(NUM_OBJ);
    let mut waiting_on = HashMap::with_capacity(NUM_TXS);
    let mut waited_on_by: HashMap<usize, Vec<usize>> = HashMap::with_capacity(NUM_TXS);
    let mut tx_tasks = JoinSet::new();
    let mut txs_ready = VecDeque::new();

    // Register all transactions with their dependencies.
    let mut nwt_guard = (&*next_write_tx).write().await;
    for (txid, tx) in txs.into_iter().enumerate() {
        let mut deps = Vec::new();
        for obj in &tx.inputs {
            if let Some(&dep) = nwt_guard.get(obj) {
                deps.push(dep);
                if let Some(list) = waited_on_by.get_mut(&dep) {
                    list.push(txid);
                }
            }
        }
        for obj in &tx.writing {
            nwt_guard.insert(*obj, txid);
        }
        if deps.is_empty() {
            txs_ready.push_back(txid);
        } else {
            waiting_on.insert(txid, deps.len());
        }
        waited_on_by.insert(txid, Vec::new());
        transactions.insert(txid, tx);
    }
    drop(nwt_guard);

    loop {
        // Run any transactions which are ready.
        /*while tx_tasks.len() < 24 && !txs_ready.is_empty() {
            let txid = txs_ready.pop_front().unwrap();*/
        for txid in txs_ready.drain(..) {
            let tx = transactions.remove(&txid).unwrap();
            let tx_locked = lock_obj(tx, &cur_obj);

            //
            let objs = &objects;
            let nwt = &*next_write_tx;
            tx_tasks.spawn(exec_tx_task(txid, tx_locked, objs, Some(nwt)));
        }

        // Join next finished transaction.
        if let Some(j) = tx_tasks.join_next().await {
            let res = j.unwrap();
            for tx_waiting in waited_on_by.remove(&res.txid).unwrap() {
                let entry = waiting_on.get_mut(&tx_waiting).unwrap();
                if let Some(num_deps) = waiting_on.get_mut(&tx_waiting) {
                    *num_deps -= 1;
                    if *num_deps == 0 {
                        txs_ready.push_back(tx_waiting);
                        waiting_on.remove(&tx_waiting);
                    }
                }
            }
        } else {
            break;
        }
    }
}

async fn execute_classic_queues(txs: Vec<Tx>) {
    let start = Instant::now();
    //static objects: Lazy<RwLock<HashMap<ObjRef, u8>>> = Lazy::new(|| RwLock::new(HashMap::new()));
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::with_capacity(NUM_OBJ));
    let mut next_write_tx: HashMap<usize, (usize, Arc<tokio::sync::watch::Sender<Option<TxResult>>>)> = HashMap::with_capacity(NUM_OBJ);
    let cur_obj = HashMap::with_capacity(NUM_OBJ);
    //let mut tx_tasks = FuturesUnordered::new();
    let mut tx_tasks = JoinSet::new();

    for (txid, tx) in txs.into_iter().enumerate() {
        // Determine dependencies of the transaction.
        let mut deps = Vec::with_capacity(tx.inputs.len());
        for obj in &tx.inputs {
            if let Some((_, dep)) = next_write_tx.get(obj) {
                deps.push(dep.subscribe());
            }
        }

        // Set the appropriate entries in `next_write_tx` to this task.
        let (snd, _) = tokio::sync::watch::channel(None);
        let snd = Arc::new(snd);
        for obj in &tx.writing {
            next_write_tx.insert(*obj, (txid, snd.clone()));
        }

        let tx_locked = lock_obj(tx, &cur_obj);

        // Spawn the transaction as a new tokio task.
        let objs = &*objects;
        //tx_tasks.push(async move {
        tx_tasks.spawn(async move {
            for mut dep in deps {
                if dep.borrow().is_some() {
                    //
                } else {
                    dep.changed().await.is_ok();
                    let _res = dep.borrow();
                }
            }
            let res = exec_tx_task(txid, tx_locked, objs, None).await;
            let _snd_res = snd.send_replace(Some(res));
        });
    }
    println!("initial spawning of tasks: {} ms", start.elapsed().as_millis());

    //while let Some(_res) = tx_tasks.next().await {
    while let Some(_res) = tx_tasks.join_next().await {
        // handle results
    }
}

async fn execute_queues(txs: Vec<Tx>) {
    let start = Instant::now();
    //static objects: Lazy<RwLock<HashMap<ObjRef, u8>>> = Lazy::new(|| RwLock::new(HashMap::new()));
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::with_capacity(NUM_OBJ));
    let mut next_write_tx: HashMap<usize, (usize, Arc<tokio::sync::watch::Sender<Option<TxResult>>>)> = HashMap::with_capacity(NUM_OBJ);
    let cur_obj = HashMap::with_capacity(NUM_OBJ);
    //let mut tx_tasks = FuturesUnordered::new();
    let mut tx_tasks = JoinSet::new();

    for (txid, tx) in txs.into_iter().enumerate() {
        // Determine dependencies of the transaction.
        let mut deps = Vec::with_capacity(tx.inputs.len());
        for obj in &tx.inputs {
            if let Some((_, dep)) = next_write_tx.get(obj) {
                deps.push(dep.subscribe());
            }
        }

        // Set the appropriate entries in `next_write_tx` to this task.
        let (snd, _) = tokio::sync::watch::channel(None);
        let snd = Arc::new(snd);
        for obj in &tx.writing {
            next_write_tx.insert(*obj, (txid, snd.clone()));
        }

        let tx_locked = lock_obj(tx, &cur_obj);

        // Spawn the transaction as a new tokio task.
        let objs = &*objects;
        //tx_tasks.push(async move {
        tx_tasks.spawn(async move {
            for mut dep in deps {
                if dep.borrow().is_some() {
                    //
                } else {
                    dep.changed().await.is_ok();
                    let _res = dep.borrow();
                }
            }
            let res = exec_tx_task(txid, tx_locked, objs, None).await;
            let _snd_res = snd.send_replace(Some(res));
        });
    }
    println!("initial spawning of tasks: {} ms", start.elapsed().as_millis());

    //while let Some(_res) = tx_tasks.next().await {
    while let Some(_res) = tx_tasks.join_next().await {
        // handle results
    }
}

async fn execute_queues2(txs: Vec<Tx>) {
    let start = Instant::now();
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::with_capacity(NUM_OBJ));
    let mut next_write_tx: HashMap<usize, (usize, Arc<Mutex<Vec<tokio::sync::oneshot::Sender<TxResult>>>>)> = HashMap::with_capacity(NUM_OBJ);
    let cur_obj = HashMap::with_capacity(NUM_OBJ);
    let mut tx_tasks = FuturesUnordered::new();
    //let mut tx_tasks = JoinSet::new();

    for (txid, tx) in txs.into_iter().enumerate() {
        // Determine dependencies of the transaction.
        let mut deps = Vec::with_capacity(tx.inputs.len());
        for obj in &tx.inputs {
            if let Some((_, dep)) = next_write_tx.get(obj) {
                let (snd, rcv) = tokio::sync::oneshot::channel();
                (*dep).lock().await.push(snd);
                deps.push(rcv);
            }
        }

        // Set the appropriate entries in `next_write_tx` to this task.
        let mut channels = None;
        for obj in &tx.writing {
            if let Some((_, c)) = next_write_tx.get(obj) {
                channels = Some(c.clone());
                break;
            }
        }
        if channels.is_none() {
            channels = Some(Arc::new(Mutex::new(Vec::with_capacity(8))));
        }
        let channels = channels.unwrap();
        for obj in &tx.writing {
            next_write_tx.insert(*obj, (txid, channels.clone()));
        }

        let tx_locked = lock_obj(tx, &cur_obj);

        // Spawn the transaction as a new tokio task.
        let objs = &*objects;
        tx_tasks.push(async move {
        //tx_tasks.spawn(async move {
            let _res = futures::future::join_all(deps);
            /*for mut dep in deps {
                let _res = dep.await;
            }*/
            let res = exec_tx_task(txid, tx_locked, objs, None).await;
            for snd in (*channels).lock().await.drain(..) {
                snd.send(res);
            }
        });
    }
    println!("initial spawning of tasks: {} ms", start.elapsed().as_millis());

    while let Some(_res) = tx_tasks.next().await {
    //while let Some(_res) = tx_tasks.join_next().await {
        // handle results
    }
}

async fn execute_queues3(txs: Vec<Tx>) {
    let start = Instant::now();
    static objects: Lazy<DashMap<ObjRef, u8>> = Lazy::new(|| DashMap::with_capacity(NUM_OBJ));
    let mut next_write_tx: HashMap<usize, (usize, Arc<Mutex<Vec<tokio::sync::mpsc::Sender<TxResult>>>>)> = HashMap::with_capacity(NUM_OBJ);
    let cur_obj = HashMap::with_capacity(NUM_OBJ);
    let mut tx_tasks = FuturesUnordered::new();
    //let mut tx_tasks = JoinSet::new();

    for (txid, tx) in txs.into_iter().enumerate() {
        // Determine dependencies of the transaction.
        let deps = tx.inputs.len();
        let (snd, mut rcv) = tokio::sync::mpsc::channel(8);
        for obj in &tx.inputs {
            if let Some((_, dep)) = next_write_tx.get(obj) {
                (*dep).lock().await.push(snd.clone());
            }
        }

        // Set the appropriate entries in `next_write_tx` to this task.
        let channels = Arc::new(Mutex::new(Vec::new()));
        for obj in &tx.writing {
            next_write_tx.insert(*obj, (txid, channels.clone()));
        }

        let tx_locked = lock_obj(tx, &cur_obj);

        // Spawn the transaction as a new tokio task.
        let objs = &*objects;
        tx_tasks.push(async move {
        //tx_tasks.spawn(async move {
            for _ in 0..deps {
                let _res = rcv.recv().await;
            }
            let res = exec_tx_task(txid, tx_locked, objs, None).await;
            for snd in (*channels).lock().await.drain(..) {
                snd.send(res).await.unwrap();
            }
        });
    }
    println!("initial spawning of tasks: {} ms", start.elapsed().as_millis());

    while let Some(_res) = tx_tasks.next().await {
    //while let Some(_res) = tx_tasks.join_next().await {
        // handle results
    }
}

fn lock_obj(tx: Tx, cur_obj: &HashMap<usize, usize>) -> TxLocked {
    let mut inputs = Vec::with_capacity(tx.inputs.len());
    for obj in tx.inputs {
        let ver = *cur_obj.get(&obj).unwrap_or(&0);
        inputs.push(ObjRef { id: obj, ver });
    }
    TxLocked { inputs, writing: tx.writing, time_ms: tx.time_ms }
}

//async fn exec_tx_task(txid: usize, tx: TxLocked, objects: &RwLock<HashMap<ObjRef, u8>>, next_write_tx: Option<&RwLock<HashMap<usize, (usize, Arc<tokio::sync::watch::Sender<TxResult>>)>>>) -> TxResult {
async fn exec_tx_task(txid: usize, tx: TxLocked, objects: &DashMap<ObjRef, u8>, next_write_tx: Option<&RwLock<HashMap<usize, usize>>>) -> TxResult {
    // Determine Lamport timestamp of output objects.
    let ver = tx.inputs.iter().map(|o| o.ver).max().unwrap_or(0) + 1;

    // Simulate some computation happening.
    let mut output = 0;
    for i in 0..4000 {
        for o in &tx.inputs {
            output = ((output as usize + o.id + o.ver) % 256) as u8;
        }
    }
    //sleep(Duration::from_millis(tx.time_ms as u64)).await;

    // Write new versions of objects.
    {
        //let mut guard = objects.write().await;
        for obj in &tx.writing {
            objects.insert(ObjRef { id: *obj, ver }, output);
        }
    }

    if let Some(next_write_tx) = next_write_tx {
        let mut guard = next_write_tx.write().await;
        for obj in &tx.writing {
            if let Some(&v) = guard.get(obj) {
                if v == txid {
                    guard.remove(obj);
                }
            }
        }
    }

    TxResult { txid }
}

/*async fn state_digest(objects: &RwLock<HashMap<ObjRef, u8>>) {
    objects.read().await.iter().ordered
}*/

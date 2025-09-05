use std::collections::HashSet;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use sui_config::local_ip_utils::*;

#[test]
fn test_concurrent_port_allocation() {
    let handles: Vec<_> = (0..10)
        .map(|_| {
            thread::spawn(|| {
                let port = get_available_port("127.0.0.1");
                assert!(port > 0);
                port
            })
        })
        .collect();

    let mut ports = Vec::new();
    for handle in handles {
        let port = handle.join().unwrap();
        ports.push(port);
    }

    // Verify all ports are unique
    let unique_ports: HashSet<_> = ports.iter().collect();
    assert_eq!(ports.len(), unique_ports.len(), "All ports should be unique");
}

#[cfg(not(msim))]
#[test]
fn test_get_available_port_with_retries_normal() {
    let port = get_available_port_with_retries("127.0.0.1", 5);
    assert!(port.is_some());
    assert!(port.unwrap() > 0);
}

#[cfg(not(msim))]
#[test]
fn test_get_available_port_with_retries_empty_host() {
    let port = get_available_port_with_retries("", 5);
    assert!(port.is_none());
}

#[cfg(not(msim))]
#[test]
fn test_get_available_port_with_retries_zero_retries() {
    let port = get_available_port_with_retries("127.0.0.1", 0);
    assert!(port.is_none());
}

#[cfg(not(msim))]
#[test]
fn test_get_available_port_with_retries_concurrent() {
    let ports = Arc::new(Mutex::new(Vec::new()));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let ports_clone = Arc::clone(&ports);
            thread::spawn(move || {
                let port = get_available_port_with_retries("127.0.0.1", 10);
                if let Some(port) = port {
                    let mut ports = ports_clone.lock().unwrap();
                    ports.push(port);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let ports = ports.lock().unwrap();
    assert!(!ports.is_empty(), "Should have allocated at least some ports");
    
    // Verify all ports are unique
    let unique_ports: HashSet<_> = ports.iter().collect();
    assert_eq!(ports.len(), unique_ports.len(), "All ports should be unique");
}

#[cfg(not(msim))]
#[test]
fn test_port_is_actually_available() {
    let port = get_available_port_with_retries("127.0.0.1", 5).unwrap();

    // Try to bind to the port to verify it's available
    let listener = TcpListener::bind(("127.0.0.1", port));
    assert!(listener.is_ok(), "Should be able to bind to the port {}", port);
}

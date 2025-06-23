package main

import (
	"context"
	"crypto/tls"
	"fmt"
	"math/rand"
	"net/http"
	"sync"
	"time"

	"github.com/Khan/genqlient/graphql"
	// "github.com/davecgh/go-spew/spew"
)

func main() {
  routines := 100
  var wg sync.WaitGroup
  wg.Add(routines)

  funcs := []func(int, chan result, *sync.WaitGroup){
    runEpochCheckpointsTXBlocks,
    runActiveValidators,
  }

  results := make(chan result)
  for i := 1; i <= routines; i++ {
    go funcs[rand.Intn(len(funcs))](i, results, &wg)
  }
  go func() {
    wg.Wait()
    close(results)
  }()
  avg := 0.0
  min := 0.0
  max := 0.0
  failed := 0.0
  for result := range results {
    avg = avg + result.duration
    if min == 0 || result.duration < min {
      min = result.duration
    }
    if result.duration > max {
      max = result.duration
    }
    if result.err != nil {
      failed = failed + 1.0
    }
  }
  success := 100.0
  if failed > 0 {
    success = failed / float64(routines) * success
  }
  avg = avg / float64(routines)
  if failed == 0.0 {
    fmt.Println("all routines finished")
  } else {
    fmt.Println("not all routines finished")
  }
  fmt.Printf("min %f avg %f max %f success rate %.2f%%\n", min, avg, max, success)
}

type result struct {
  id       int
  duration float64
  err      error
}

func runActiveValidators(id int, out chan result, wg *sync.WaitGroup) {
  defer wg.Done()
  startTime := time.Now()
  ctx := context.Background()
  tr := &http.Transport{
    TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
  }
  hc := &http.Client{Transport: tr}
  gc := graphql.NewClient("https://sui-testnet.mystenlabs.com/graphql", hc)
  _, err := active_validators(ctx, gc)
  stop := time.Since(startTime).Seconds()
  if err != nil {
    out <- result{id: id, duration: stop, err: err}
    fmt.Printf("runActiveValidators %d: %v\n", id, err)
    return
  }
  out <- result{id: id, duration: stop}
  fmt.Printf("runActiveValidators %d: finished %.2f sec \n", id, stop)
}

func runEpochCheckpointsTXBlocks(id int, out chan result, wg *sync.WaitGroup) {
  defer wg.Done()
  startTime := time.Now()
  ctx := context.Background()
  tr := &http.Transport{
    TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
  }
  hc := &http.Client{Transport: tr}
  gc := graphql.NewClient("https://sui-testnet.mystenlabs.com/graphql", hc)
  _, err := epoch_checkpoints_tx_blocks(ctx, gc)
  stop := time.Since(startTime).Seconds()
  if err != nil {
    out <- result{id: id, duration: stop, err: err}
    fmt.Printf("runEpochCheckpointsTXBlocks %d: %v\n", id, err)
    return
  }
  out <- result{id: id, duration: stop}
  fmt.Printf("runEpochCheckpointsTXBlocks %d: finished %.2f sec \n", id, time.Since(startTime).Seconds())
}

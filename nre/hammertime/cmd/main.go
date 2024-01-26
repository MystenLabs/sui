package main

import (
	"context"
	"crypto/tls"
	"fmt"
	"math/rand"
	"net/http"
	"sync"

	"github.com/Khan/genqlient/graphql"
	// "github.com/davecgh/go-spew/spew"
)

func main() {
  routines := 100
  var wg sync.WaitGroup
  wg.Add(routines)

  funcs := []func(int, *sync.WaitGroup){
    runEpochCheckpointsTXBlocks,
    runActiveValidators,
  }

  for i:=1; i <= routines; i++ {
    go funcs[rand.Intn(len(funcs))](i, &wg)
  }
  wg.Wait()
  fmt.Println("all routines finished")
}


func runActiveValidators(id int, wg *sync.WaitGroup) {
  defer wg.Done()

  ctx := context.Background()
  tr := &http.Transport{
		TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
	}
  hc := &http.Client{Transport: tr}
  gc := graphql.NewClient("https://sui-testnet.mystenlabs.com/graphql", hc)
  _, err := active_validators(ctx, gc)
  if err != nil {
    fmt.Printf("runActiveValidators: %v\n", err)
    return
  }

  fmt.Printf("runActiveValidators %d: finished\n", id)
}

func runEpochCheckpointsTXBlocks(id int, wg *sync.WaitGroup) {
  defer wg.Done()

  ctx := context.Background()
  tr := &http.Transport{
		TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
	}
  hc := &http.Client{Transport: tr}
  gc := graphql.NewClient("https://sui-testnet.mystenlabs.com/graphql", hc)
  _, err := epoch_checkpoints_tx_blocks(ctx, gc)
  if err != nil {
    fmt.Printf("runEpochCheckpointsTXBlocks: %v\n", err)
    return
  }
  fmt.Printf("runEpochCheckpointsTXBlocks %d: finished\n", id)
}
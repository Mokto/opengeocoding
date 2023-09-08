package graceful

import (
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"
)

// Manager takes care of graceful shutdown
type Manager struct {
	callbacks   []func()
	shutdownWg  sync.WaitGroup
	processesWg sync.WaitGroup
}

// Start initialized the graceful manager
func Start() *Manager {

	manager := &Manager{
		callbacks: []func(){},
	}

	manager.shutdownWg.Add(1)

	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-c
		// sig is a ^C, handle it
		log.Println("Graceful shutdown started...")

		for _, callback := range manager.callbacks {
			go func(callback func()) {
				callback()
				manager.processesWg.Done()
			}(callback)
		}

		manager.processesWg.Wait()

		log.Println("Graceful shutdown Done.")
		manager.shutdownWg.Done()
	}()

	return manager
}

// Wait waits for all subprocesses to be done
func (manager *Manager) Wait() {
	manager.shutdownWg.Wait()
}

// OnShutdown is triggered when a shutdown starts. When the result is returned, it means the subprocess is ready to be shutdown
func (manager *Manager) OnShutdown(callback func()) {
	manager.processesWg.Add(1)
	manager.callbacks = append(manager.callbacks, callback)
}

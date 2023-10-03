package elasticsearch

import (
	"log"
	"net/http"
	"strconv"
	"time"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8"
	"github.com/hashicorp/go-retryablehttp"
	"github.com/logrusorgru/aurora/v4"
)

// Elasticsearch contains everything to access elasticsearch
type Elasticsearch struct {
	client      *elasticsearch.Client
	maxReplicas int
	maxShards   int
}

// Config Elasticsearch
type Config struct {
	// Logger   elastictransport.Logger
	Password string
	URL      string
	User     string
	// BatchCount    int
	// BatchTimeout  int
	// MaxReplicas   int
	// MaxShards     int
	// Ephemeral     bool
	// NoInitialPing bool
}

// New initializes a new elasticsearch connection
func New(config Config, transports ...http.RoundTripper) (*Elasticsearch, error) {

	var customTransport http.RoundTripper
	if len(transports) == 1 {
		customTransport = transports[0]
	} else {
		client := retryablehttp.NewClient()
		client.Logger = nil
		client.RetryMax = 3
		client.RetryWaitMin = time.Millisecond * 200
		client.RetryWaitMax = time.Millisecond * 500
		customTransport = client.HTTPClient.Transport
		t := customTransport.(*http.Transport).Clone()
		t.MaxIdleConnsPerHost = 500
		t.MaxIdleConns = 500

		customTransport = t
	}

	return newEs(config, 0, customTransport)
}

func newEs(config Config, retry int, customTransport http.RoundTripper) (*Elasticsearch, error) {

	log.Println("Connecting to elasticsearch ", config.URL)

	client, err := elasticsearch.NewClient(elasticsearch.Config{
		Username:  config.User,
		Password:  config.Password,
		Addresses: []string{config.URL},
		Transport: customTransport,
		// Logger:                  config.Logger,
		EnableCompatibilityMode: true,
	})

	if err != nil {
		return nil, errors.Wrap(err)
	}

	// if config.MaxShards == 0 {
	// 	config.MaxShards = 100000
	// }

	es := Elasticsearch{
		client: client,
		// maxReplicas: config.MaxReplicas,
		// maxShards:   config.MaxShards,
	}

	err = es.GetClusterInfo()
	if err != nil {
		if retry < 15 {
			log.Println(aurora.Cyan("Retry connecting to elasticsearch after " + strconv.Itoa(retry) + " seconds..."))
			time.Sleep(time.Duration(retry) * time.Second)
			return newEs(config, retry+1, customTransport)
		}
		return nil, errors.Wrap(err)
	}

	return &es, nil
}

func (es *Elasticsearch) getIndexName(indexName string) string {
	return indexName
}

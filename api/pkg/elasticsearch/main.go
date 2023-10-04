package elasticsearch

import (
	"log"
	"net/http"
	"os"
	"strconv"
	"time"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8"
	"github.com/hashicorp/go-retryablehttp"
	"github.com/logrusorgru/aurora/v4"
)

// Elasticsearch contains everything to access elasticsearch
type Elasticsearch struct {
	client *elasticsearch.Client
}

type Config struct {
	Password string
	URL      string
	User     string
}

func InitDatabase() *Elasticsearch {

	elasticsearch_endpoint := os.Getenv("ELASTICSEARCH_ENDPOINT")
	elasticsearch_user := os.Getenv("ELASTICSEARCH_USER")
	elasticsearch_password := os.Getenv("ELASTICSEARCH_PASSWORD")
	if elasticsearch_endpoint == "" {
		elasticsearch_endpoint = "http://localhost:9200"
	}

	elasticsearch, err := newConnection(Config{
		Password: elasticsearch_password,
		URL:      elasticsearch_endpoint,
		User:     elasticsearch_user,
	})
	if err != nil {
		panic(err)
	}

	return elasticsearch
}

// New initializes a new elasticsearch connection
func newConnection(config Config, transports ...http.RoundTripper) (*Elasticsearch, error) {

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

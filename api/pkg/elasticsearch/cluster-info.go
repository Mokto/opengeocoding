package elasticsearch

import (
	"encoding/json"
	"fmt"
	"log"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8"
)

// GetClusterInfo logs elasticserach infos
func (es *Elasticsearch) GetClusterInfo() error {
	var r map[string]interface{}

	res, err := es.client.Info()
	if err != nil {
		return errors.Wrap(err)
	}
	defer res.Body.Close()
	// Check response status
	if res.IsError() {
		return errors.Wrap(errors.New(fmt.Sprintf("Error: %s", res.String())))
	}
	// Deserialize the response into a map.
	if err := json.NewDecoder(res.Body).Decode(&r); err != nil {
		return errors.Wrap(err)
	}
	// Print client and server version numbers.
	log.Println(
		"Connected to Elasticsearch",
		fmt.Sprintf("Client: %s,", elasticsearch.Version),
		fmt.Sprintf("Server: %s", r["version"].(map[string]interface{})["number"]),
	)

	return nil
}

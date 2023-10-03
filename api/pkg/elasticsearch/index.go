package elasticsearch

import (
	"bytes"
	"context"
	"strings"
	"time"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8/esapi"
	"github.com/tidwall/gjson"
)

// IndexExists verify is an index exists
func (es *Elasticsearch) IndexExists(name string) (bool, error) {

	req := esapi.IndicesExistsRequest{Index: []string{es.getIndexName(name)}}

	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return false, errors.Wrap(err)
	}
	defer res.Body.Close()

	if res.StatusCode == 200 {
		return true, nil
	}
	if res.StatusCode == 404 {
		return false, nil
	}

	return false, errors.New("status code 500 on HEAD /" + name)
}

// CreateIndex create an index with some mappings
func (es *Elasticsearch) CreateIndex(name string, mappings string) error {
	req := esapi.IndicesCreateRequest{Index: es.getIndexName(name), Body: strings.NewReader(mappings)}
	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return errors.Wrap(err)
	}
	defer res.Body.Close()

	if res.StatusCode != 200 {
		buf := new(bytes.Buffer)
		buf.ReadFrom(res.Body)
		result := buf.String()
		return errors.New("Status code != 200 on create index " + name + " : " + result)
	}

	return nil
}

// RefreshIndex refreshes the index
func (es *Elasticsearch) RefreshIndex(name string) error {
	req := esapi.IndicesRefreshRequest{Index: []string{es.getIndexName(name)}}
	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return errors.Wrap(err)
	}
	defer res.Body.Close()

	if res.StatusCode != 200 {
		buf := new(bytes.Buffer)
		buf.ReadFrom(res.Body)
		result := buf.String()
		return errors.New("Status code != 200 on refreshIndex "+name).AddContext("status", res.Status()).AddContext("body", result)
	}

	return nil
}

// CreateIndexIfNotExists create the index, but only if it does not exist
func (es *Elasticsearch) CreateIndexIfNotExists(name string, mappings string) error {
	exists, err := es.IndexExists(name)
	if err != nil {
		return errors.Wrap(err)
	}
	if exists {
		return nil
	}

	err = es.CreateIndex(name, mappings)
	if err != nil {
		return errors.Wrap(err)
	}

	return nil
}

// DeleteIndex deletes an index
func (es *Elasticsearch) DeleteIndex(name string) error {
	req := esapi.IndicesDeleteRequest{Index: []string{es.getIndexName(name)}}
	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return errors.Wrap(err)
	}
	defer res.Body.Close()

	if res.StatusCode != 200 {
		buf := new(bytes.Buffer)
		buf.ReadFrom(res.Body)
		result := buf.String()
		return errors.New("Status code != 200 on deleteIndex " + name + " " + result)
	}

	return nil
}

func (es *Elasticsearch) GetIndexSettings(name string) (string, error) {
	trueVal := true
	req := esapi.IndicesGetSettingsRequest{Index: []string{es.getIndexName(name)}, IncludeDefaults: &trueVal}
	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return "", errors.Wrap(err)
	}
	defer res.Body.Close()

	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	result := buf.String()

	if res.StatusCode != 200 {
		return "", errors.New("Status code != 200 on getSettings " + name + " " + result)
	}

	return gjson.Get(result, es.getIndexName(name)).String(), nil
}

func (es *Elasticsearch) GetRefreshInterval(indexName string) (time.Duration, error) {
	settings, err := es.GetIndexSettings(indexName)
	if err != nil {
		panic(err)
	}
	if ri := gjson.Get(settings, "settings.index.refresh_interval").String(); ri != "" {
		return time.ParseDuration(ri)
	}
	return time.ParseDuration(gjson.Get(settings, "defaults.index.refresh_interval").String())
}

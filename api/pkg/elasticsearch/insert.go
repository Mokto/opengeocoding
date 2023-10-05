package elasticsearch

import (
	"bytes"
	"context"
	"fmt"
	"strconv"
	"strings"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8/esapi"
	"github.com/tidwall/gjson"
	"github.com/tidwall/sjson"
)

type InsertOptions struct {
	Routing string
	Version int64
}

// InsertDocument adds a new document in elasticsearhc
func (es *Elasticsearch) InsertDocument(ctx context.Context, indexName string, id string, content string, options ...InsertOptions) error {

	Options := InsertOptions{}
	if len(options) == 1 {
		Options = options[0]
	}

	var version *int
	if Options.Version != 0 {
		optionVersion := int(Options.Version)
		version = &optionVersion
	}

	req := esapi.IndexRequest{
		Index:      es.getIndexName(indexName),
		DocumentID: id,
		Body:       strings.NewReader(content),
		Refresh:    "true",
		Routing:    Options.Routing,
		Version:    version,
	}

	res, err := req.Do(ctx, es.client)
	if err != nil {
		return errors.Wrap(err)
	}

	defer res.Body.Close()
	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	result := buf.String()

	if res.StatusCode != 200 && res.StatusCode != 201 {
		return errors.New("res.StatusCode (" + strconv.Itoa(res.StatusCode) + ") != 20x, " + result)
	}

	return nil
}

// BulkInsertDocuments adds multiple documents at once. The key of the map corresponds to _id, value is the elasticsearch json
func (es *Elasticsearch) BulkInsertDocuments(ctx context.Context, indexName string, documentsMap map[string]string, routings ...string) error {
	routing := ""
	if len(routings) > 0 {
		routing = `,"routing": "` + routings[0] + `"`
	}

	var buffer bytes.Buffer

	if len(documentsMap) == 0 {
		return nil
	}

	for id, document := range documentsMap {
		meta := []byte(fmt.Sprintf(`{ "index" : { "_id" : "%s"%s } }%s`, id, routing, "\n"))
		dataBytes := []byte(strings.Replace(document, "\n", "", -1) + "\n")

		buffer.Grow(len(meta) + len(dataBytes))
		buffer.Write(meta)
		buffer.Write(dataBytes)
	}

	res, err := es.client.Bulk(
		bytes.NewReader(buffer.Bytes()),
		es.client.Bulk.WithIndex(es.getIndexName(indexName)),
		es.client.Bulk.WithContext(context.Background()),
	)

	if err != nil {
		return errors.Wrap(err)
	}

	defer res.Body.Close()

	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	result := buf.String()

	if res.StatusCode != 200 && res.StatusCode != 201 {
		return errors.New("res.StatusCode (" + strconv.Itoa(res.StatusCode) + ") != 20x, " + result)
	}
	if gjson.Get(result, "errors").Bool() {
		errorMsg := "Bulk insert has some issues: \n"
		errorsMap := map[string]string{}
		for _, item := range gjson.Get(result, "items").Array() {
			if item.Get("index.error").String() != "" {
				errorMsg += item.String() + "\n"
				id := item.Get("index._id").String()
				errorsMap[id] = documentsMap[id]
			}

		}
		return errors.New(errorMsg).AddContext("documents", errorsMap)
	}

	return nil
}

func (es *Elasticsearch) BulkInsertDocumentsWithRouting(ctx context.Context, indexName string, documentsMap map[string]string, routings ...string) error {

	var buffer bytes.Buffer

	if len(documentsMap) == 0 {
		return nil
	}

	for id, document := range documentsMap {
		routing := gjson.Get(document, "_routing").String()
		document, _ = sjson.Delete(document, "_routing")

		meta := []byte(fmt.Sprintf(`{ "index" : { "_id" : "%s"%s } }%s`, id, `,"routing": "`+routing+`"`, "\n"))
		dataBytes := []byte(strings.Replace(document, "\n", "", -1) + "\n")

		buffer.Grow(len(meta) + len(dataBytes))
		buffer.Write(meta)
		buffer.Write(dataBytes)
	}

	res, err := es.client.Bulk(
		bytes.NewReader(buffer.Bytes()),
		es.client.Bulk.WithIndex(es.getIndexName(indexName)),
		es.client.Bulk.WithContext(context.Background()),
	)

	if err != nil {
		return errors.Wrap(err)
	}

	defer res.Body.Close()

	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	result := buf.String()

	if res.StatusCode != 200 && res.StatusCode != 201 {
		fmt.Println(result)
		return errors.New("res.StatusCode (" + strconv.Itoa(res.StatusCode) + ") != 20x, " + result)
	}
	if gjson.Get(result, "errors").Bool() {
		errorMsg := "Bulk insert has some issues: \n"
		errorsMap := map[string]string{}
		for _, item := range gjson.Get(result, "items").Array() {
			if item.Get("index.error").String() != "" {
				errorMsg += item.String() + "\n"
				id := item.Get("index._id").String()
				errorsMap[id] = documentsMap[id]
			}

		}
		return errors.New(errorMsg).AddContext("documents", errorsMap)
	}

	return nil
}

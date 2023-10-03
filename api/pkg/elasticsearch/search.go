package elasticsearch

import (
	"bytes"
	"context"
	"strconv"
	"strings"

	"geocoding/pkg/errors"

	"github.com/elastic/go-elasticsearch/v8/esapi"
	"github.com/tidwall/gjson"
	"github.com/tidwall/sjson"
)

// SearchParams defines how to search
type SearchParams struct {
	Body           string
	SourceIncludes []string
	Sort           []string
	From           int
	Size           int
	TrackTotalHits bool
}

type SearchAllParams struct {
	Callback func(results []string)
	Body     *SearchBody
	SearchParams
}

// SearchMetadata contains the next scroll id but also other useful information (ie total results)
type SearchMetadata struct {
	NextScrollID string
	Total        uint64
}

type AggregatedField struct {
	Name  string
	Field string
}

// AggregatedStatisticsOptions Model
type AggregatedStatisticsOptions struct {
	NestedPath string
	Fields     []AggregatedField
}

// SearchOne search for one result and returns it
func (es *Elasticsearch) SearchOne(ctx context.Context, indexName string, searchParams SearchParams) (result string, err error) {
	searchParams.Size = 1
	res, err := es.search(indexName, searchParams)
	if err != nil {
		return "", errors.Wrap(err)
	}

	return gjson.Get(res, "hits.hits.0").String(), nil
}

// SearchMany search and returns a list of results
func (es *Elasticsearch) SearchMany(ctx context.Context, indexName string, searchParams SearchParams) (results []string, metadata *SearchMetadata, err error) {

	res, err := es.search(indexName, searchParams)
	if err != nil {
		return nil, nil, errors.Wrap(err)
	}

	results = make([]string, 0)

	for _, val := range gjson.Get(res, "hits.hits").Array() {
		results = append(results, val.String())
	}

	return results, &SearchMetadata{
		NextScrollID: gjson.Get(res, "_scroll_id").String(),
		Total:        uint64(gjson.Get(res, "hits.total.value").Int()),
	}, nil
}

func (es *Elasticsearch) search(indexName string, searchParams SearchParams) (body string, err error) {

	var req esapi.Request

	version := true
	size := &searchParams.Size
	if searchParams.Size == 0 {
		size = nil
	}
	req = esapi.SearchRequest{
		Index:          []string{es.getIndexName(indexName)},
		TrackTotalHits: searchParams.TrackTotalHits,
		Body:           strings.NewReader(searchParams.Body),
		Sort:           searchParams.Sort,
		SourceIncludes: searchParams.SourceIncludes,
		Size:           size,
		From:           &searchParams.From,
		Version:        &version,
	}

	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return "", errors.Wrap(err)
	}

	defer res.Body.Close()
	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	resStr := buf.String()

	if res.StatusCode != 200 {
		return "", errors.New("statusCode "+strconv.Itoa(res.StatusCode)+"!= 200, res"+resStr).AddContext("body", searchParams.Body)
	}

	return resStr, nil
}

// GetAggregatedStatistics search for aggregations and return it
func (es *Elasticsearch) GetAggregatedStatistics(ctx context.Context, indexName string, options AggregatedStatisticsOptions, searchBody *SearchBody) (result string, err error) {

	val := "{}"
	if options.NestedPath != "" {
		val, _ = sjson.SetRaw(val, "query", searchBody.body)
		val, _ = sjson.Set(val, "aggs.aggregations.nested.path", "crmDeals")

		for _, field := range options.Fields {
			val, _ = sjson.Set(val, "aggs.aggregations.aggs."+field.Name+".stats.field", field.Field)
		}
	} else {
		for _, field := range options.Fields {
			val, _ = sjson.Set(val, "aggs."+field.Name+".stats.field", field.Field)
		}
		val, _ = sjson.SetRaw(val, "query", searchBody.body)
	}

	searchBody.body = val

	var req esapi.Request = esapi.SearchRequest{
		Body:  strings.NewReader(searchBody.body),
		Index: []string{es.getIndexName(indexName)},
	}

	res, err := req.Do(context.Background(), es.client)
	if err != nil {
		return "", errors.Wrap(err)
	}

	defer res.Body.Close()
	buf := new(bytes.Buffer)
	buf.ReadFrom(res.Body)
	resStr := buf.String()

	return gjson.Get(resStr, "aggregations").String(), nil
}

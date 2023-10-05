package elasticsearch

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/logrusorgru/aurora/v4"
	"github.com/tidwall/sjson"
)

const maxTermsCount = 65536

// SearchBody contains the ES body
// TODO rename to more common name as use not only in search
type SearchBody struct {
	body            string // TODO shouldn't be string
	sort            string
	collapse        string
	searchAfter     string
	collapseSources []string
	minScore        float32
}

// NewSearchBody allows to generate an ES body easily
func NewSearchBody() *SearchBody {
	return &SearchBody{
		body: `{}`,
		sort: ``,
	}
}

// Body returns the main body in query
func (searchBody *SearchBody) Body() string {
	sort := ""
	if searchBody.sort != "" {
		if searchBody.body != "{}" {
			sort = ", "
		}
		sort = sort + `"sort":` + searchBody.sort
	}
	minScore := ""
	if searchBody.minScore != 0 {
		if searchBody.body != "{}" {
			minScore = ", "
		}
		minScore = minScore + fmt.Sprintf(`"min_score":%f`, searchBody.minScore)
	}
	collapse := ""
	if searchBody.collapse != "" {
		if searchBody.body != "{}" || searchBody.sort != "" {
			collapse = ", "
		}
		sourceInclude := ""
		if len(searchBody.collapseSources) > 0 {
			sourceInclude = `"_source": {"include": ["` + strings.Join(searchBody.collapseSources, `","`) + `"]},`
		}
		collapse = collapse + `"collapse":{"field":"` + searchBody.collapse + `", "inner_hits": {"name": "hit", ` + sourceInclude + `"size": 10000}}`
	}
	searchAfter := ""
	if searchBody.searchAfter != "" {
		if searchBody.body != "{}" || searchBody.sort != "" || searchBody.collapse != "" {
			searchAfter = ", "
		}
		searchAfter = searchAfter + `"search_after":[` + searchBody.searchAfter + `]`
	}
	if searchBody.body == "{}" {
		return "{" + sort + collapse + searchAfter + "}"
	}
	return `{"query":` + searchBody.body + sort + collapse + searchAfter + minScore + `}`
}

func (searchBody *SearchBody) MinScore(score float32) *SearchBody {
	searchBody.minScore = score
	return searchBody
}

// WithParentID search for documents with a specific parent id
func (searchBody *SearchBody) WithParentID(childName string, parentID string) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "parent_id", `{
		"type": "`+childName+`",
		"id": "`+parentID+`"
	}`)
	searchBody.body = val
	return searchBody
}

// MinimumShouldMatch adds minimum_should_match to bool
func (searchBody *SearchBody) MinimumShouldMatch(value interface{}) *SearchBody {
	val, _ := sjson.Set(searchBody.body, "bool.minimum_should_match", value)
	searchBody.body = val
	return searchBody
}

// MustTerm adds a new element to the must array
func (searchBody *SearchBody) MustTerm(termKey string, termValue interface{}) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.must.-1.term."+termKey+".value", termValue)
	searchBody.body = val
	return searchBody
}

// MustTerms adds a new element to the must array
func (searchBody *SearchBody) MustTerms(termKey string, termValue []string) *SearchBody {
	end := len(termValue)
	if end > maxTermsCount {
		end = maxTermsCount
	}
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.must.-1.terms."+termKey, termValue[:end])
	searchBody.body = val
	return searchBody
}

// Filter sets a filter
func (searchBody *SearchBody) FilterTerm(termKey string, termValue interface{}) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.filter.-1.term."+termKey, termValue)
	searchBody.body = val
	return searchBody
}

// FilterRange checks for a time range
func (searchBody *SearchBody) FilterRange(termKey string, lowerThan string, greaterThan string) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val := "{}"
	if lowerThan != "" {
		val, _ = sjson.Set(val, "lt", lowerThan)
	}

	if greaterThan != "" {
		val, _ = sjson.Set(val, "gte", greaterThan)
	}

	val, _ = sjson.SetRaw(searchBody.body, "bool.filter.-1.range."+termKey, val)
	searchBody.body = val
	return searchBody
}

// ShouldTerm adds a new element to the should array
func (searchBody *SearchBody) ShouldTerm(termKey string, termValue interface{}) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.should.-1.term."+termKey+".value", termValue)
	searchBody.body = val
	return searchBody
}

// ShouldRange checks for a time range
func (searchBody *SearchBody) ShouldRange(termKey string, lowerThan string, greaterThan string) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val := "{}"
	if lowerThan != "" {
		val, _ = sjson.Set(val, "lt", lowerThan)
	}

	if greaterThan != "" {
		val, _ = sjson.Set(val, "gte", greaterThan)
	}

	val, _ = sjson.SetRaw(searchBody.body, "bool.should.-1.range."+termKey, val)
	searchBody.body = val
	return searchBody
}

// MustRange checks for a time range
func (searchBody *SearchBody) MustRange(termKey string, lowerThan string, greaterThan string) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val := "{}"
	if lowerThan != "" {
		val, _ = sjson.Set(val, "lt", lowerThan)
	}

	if greaterThan != "" {
		val, _ = sjson.Set(val, "gte", greaterThan)
	}

	val, _ = sjson.SetRaw(searchBody.body, "bool.must.-1.range."+termKey, val)
	searchBody.body = val
	return searchBody

}

// FilterTerms sets a filter on multiple terms
func (searchBody *SearchBody) FilterTerms(termKey string, values []string) *SearchBody {
	end := len(values)
	if end > maxTermsCount {
		end = maxTermsCount
	}
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.filter.-1.terms."+termKey, values[:end])
	searchBody.body = val
	return searchBody
}

// FilterMatch sets a filter on multiple terms
func (searchBody *SearchBody) FilterMatch(termKey string, value interface{}) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.filter.-1.match."+termKey, value)
	searchBody.body = val
	return searchBody
}

func (searchBody *SearchBody) FilterMatchPhrase(termKey string, value interface{}) *SearchBody {
	termKey = strings.ReplaceAll(termKey, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.filter.-1.match_phrase."+termKey, value)
	searchBody.body = val
	return searchBody
}

// ShouldCustom add should with a custom sjson key
func (searchBody *SearchBody) ShouldCustom(body *SearchBody) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "bool.should.-1", body.body)
	searchBody.body = val
	return searchBody
}

// ShouldCustomRaw add should with a custom sjson key
func (searchBody *SearchBody) ShouldCustomRaw(key string, value string) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "bool.should.-1."+key, value)
	searchBody.body = val
	return searchBody
}

// ShouldExists confirms a field exists sets a filter
func (searchBody *SearchBody) ShouldExists(fieldName string) *SearchBody {
	val, _ := sjson.Set(searchBody.body, "bool.should.-1.exists.field", fieldName)
	searchBody.body = val
	return searchBody
}

// FilterCustom add filter with a custom body
func (searchBody *SearchBody) FilterCustom(body *SearchBody) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "bool.filter.-1", body.body)
	searchBody.body = val
	return searchBody
}

// FilterCustomRaw add should with a custom sjson key
func (searchBody *SearchBody) FilterCustomRaw(key string, value string) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "bool.filter.-1."+key, value)
	searchBody.body = val
	return searchBody
}

// FilterExists confirms a field exists sets a filter
func (searchBody *SearchBody) FilterExists(fieldName string) *SearchBody {
	val, _ := sjson.Set(searchBody.body, "bool.filter.-1.exists.field", fieldName)
	searchBody.body = val
	return searchBody
}

// FilterNested nested filter
func (searchBody *SearchBody) FilterNested(path string, body *SearchBody, innerHitsValues ...string) *SearchBody {
	val, _ := sjson.Set("{}", "path", path)
	val, _ = sjson.SetRaw(val, "query", body.body)
	if len(innerHitsValues) != 0 {
		val, _ = sjson.Set(val, "inner_hits._source", innerHitsValues)
	}
	val, _ = sjson.SetRaw(searchBody.body, "bool.filter.-1.nested", val)
	searchBody.body = val
	return searchBody
}

// MustNotExists looks for field that don't exist
func (searchBody *SearchBody) MustNotExists(key string) *SearchBody {
	val, _ := sjson.Set(searchBody.body, "bool.must_not.-1.exists.field", key)
	searchBody.body = val
	return searchBody
}

// MustNotTerms looks for field that don't match an array of elements
func (searchBody *SearchBody) MustNotTerms(key string, values []string) *SearchBody {
	end := len(values)
	if end > maxTermsCount {
		end = maxTermsCount
	}
	key = strings.ReplaceAll(key, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.must_not.-1.terms."+key, values[:end])
	searchBody.body = val
	return searchBody
}

// MustNotTerm looks for field that don not match with string
func (searchBody *SearchBody) MustNotTerm(key string, value interface{}) *SearchBody {

	key = strings.ReplaceAll(key, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.must_not.-1.term."+key, value)
	searchBody.body = val
	return searchBody
}

// ShouldMatch add should match to the body
func (searchBody *SearchBody) ShouldMatch(key string, value interface{}) *SearchBody {
	key = strings.ReplaceAll(key, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.should.-1.match."+key, value)
	searchBody.body = val
	return searchBody
}

func (searchBody *SearchBody) ShouldMatchPhrase(key string, value interface{}) *SearchBody {
	key = strings.ReplaceAll(key, ".", "\\.")
	val, _ := sjson.Set(searchBody.body, "bool.should.-1.match_phrase."+key, value)
	searchBody.body = val
	return searchBody
}

// HasChildCustomOptions type
type HasChildCustomOptions struct {
	ScoreMode string
}

// HasChildCustom adds has_child to query
func (searchBody *SearchBody) HasChildCustom(fieldType string, query *SearchBody, options ...HasChildCustomOptions) *SearchBody {
	Options := HasChildCustomOptions{}
	if len(options) == 1 {
		Options = options[0]
	}
	scoreMode := ""
	if Options.ScoreMode != "" {
		scoreMode = `,"score_mode":"` + Options.ScoreMode + `"`
	}
	val, _ := sjson.SetRaw(searchBody.body, "has_child", `{"type":"`+fieldType+`","query":`+query.body+scoreMode+`}`)
	searchBody.body = val
	return searchBody
}

// WildCard adds wildcard query
func (searchBody *SearchBody) WildCard(field string, value string, boost float32) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "wildcard", `{"`+field+`":{"value":"`+strings.ToLower(value)+`","boost":`+fmt.Sprintf("%f", boost)+`}}`)
	searchBody.body = val
	return searchBody
}

// Sort sets sorting
func (searchBody *SearchBody) Sort(key string, order ...string) *SearchBody {
	if len(order) > 0 {
		if searchBody.sort == "" {
			searchBody.sort = "{}"
		}
		key = strings.ReplaceAll(key, ".", "\\.")
		val, _ := sjson.Set(searchBody.sort, key+".order", order[0])
		searchBody.sort = val
	} else {
		searchBody.sort = `["` + key + `"]`
	}

	return searchBody
}

// FunctionScoreRandom randomize passed search body
func (searchBody *SearchBody) FunctionScoreRandom(body *SearchBody) *SearchBody {
	randomScore := SearchBody{}
	val, _ := sjson.SetRaw(randomScore.body, "function_score.query", body.body)
	val, _ = sjson.SetRaw(val, "function_score.random_score", `{}`)
	val, _ = sjson.Set(val, "function_score.boost_mode", "avg")

	searchBody.body = val
	return searchBody
}

// FunctionScoreScript adds a score function script
func (searchBody *SearchBody) FunctionScoreScript(script string) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "function_score.script_score.script", `"`+script+`"`)
	searchBody.body = val
	return searchBody
}

// MatchAll adds match_all to quesry
func (searchBody *SearchBody) MatchAll(value string) *SearchBody {
	val, _ := sjson.SetRaw(searchBody.body, "match_all", value)
	searchBody.body = val
	return searchBody
}

// Debug allows to print the generated json
func (searchBody *SearchBody) Debug() {
	var out bytes.Buffer
	err := json.Indent(&out, []byte(searchBody.Body()), "", "  ")
	if err != nil {
		fmt.Println(aurora.Red(searchBody))
		panic(err)
	}
	fmt.Println(out.String())
}

// ShouldNested nested filter
func (searchBody *SearchBody) ShouldNested(path string, body *SearchBody, innerHitsValues ...string) *SearchBody {
	val, _ := sjson.Set("{}", "path", path)
	val, _ = sjson.SetRaw(val, "query", body.body)
	if len(innerHitsValues) != 0 {
		val, _ = sjson.Set(val, "inner_hits._source", innerHitsValues)
	}
	val, _ = sjson.SetRaw(searchBody.body, "bool.should.-1.nested", val)
	searchBody.body = val
	return searchBody
}

// Collapse group results by a key
func (searchBody *SearchBody) Collapse(fieldName string, sources ...string) *SearchBody {
	searchBody.collapse = fieldName
	searchBody.collapseSources = sources
	return searchBody
}

// SearchAfter search after a specific id
func (searchBody *SearchBody) SearchAfter(id string) *SearchBody {
	searchBody.searchAfter = id
	return searchBody
}

package download

import (
	"bytes"
	"encoding/csv"
)

func ReadCsv(b []byte) ([][]string, error) {
	r := csv.NewReader(bytes.NewReader(b))
	r.Comma = '\t'

	records, err := r.ReadAll()
	if err != nil {
		return nil, err
	}

	return records, nil
}

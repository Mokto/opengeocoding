package write

import (
	"go/format"
	"log"
	"os"
)

func FormatAndWrite(filepath string, content string) error {

	formatted, err := format.Source([]byte(content))
	if err != nil {
		log.Fatal(err)
	}

	err = os.WriteFile(filepath, formatted, 0644)
	if err != nil {
		return err
	}

	return nil
}

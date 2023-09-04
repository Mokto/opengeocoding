package download

import (
	"fmt"
	"io"
	"net/http"
	"os"
)

func DownloadFile(filepath string, url string) (result []byte, err error) {

	err = os.MkdirAll("./data", os.ModePerm)
	if err != nil {
		return nil, err
	}

	if !fileExists(filepath) {
		fmt.Println("Downloading...")

		// Create the file
		out, err := os.Create(filepath)
		if err != nil {
			return nil, err
		}
		defer out.Close()

		// Get the data
		resp, err := http.Get(url)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()

		// Check server response
		if resp.StatusCode != http.StatusOK {
			return nil, fmt.Errorf("bad status: %s", resp.Status)
		}

		// Writer the body to file
		_, err = io.Copy(out, resp.Body)
		if err != nil {
			return nil, err
		}

	}

	byteValue, _ := os.ReadFile(filepath)
	if err != nil {
		return nil, err
	}

	return byteValue, nil

}

func fileExists(filename string) bool {
	_, err := os.Stat(filename)
	if err != nil {
		if os.IsNotExist(err) {
			return false
		}
	}
	return true
}

package download

import (
	"archive/zip"
	"fmt"
	"io"
	"os"
)

func DownloadAndUnzip(zipFilepath string, zipUrl string, filepath string, fileNameInsideArchive string) ([]byte, error) {
	err := os.MkdirAll("./data", os.ModePerm)
	if err != nil {
		return nil, err
	}

	if !fileExists(filepath) {
		fmt.Println("Downloading...")

		_, err := DownloadFile(zipFilepath, zipUrl)
		if err != nil {
			return nil, err
		}
		zipReader, err := zip.OpenReader(zipFilepath)
		if err != nil {
			return nil, err
		}
		defer zipReader.Close()

		for _, file := range zipReader.File {
			if file.Name == fileNameInsideArchive {
				fmt.Println("Extracting...")

				// Create the file
				out, err := os.Create(filepath)
				if err != nil {
					return nil, err
				}
				defer out.Close()

				reader, err := file.Open()
				if err != nil {
					return nil, err
				}

				_, err = io.Copy(out, reader)
				if err != nil {
					return nil, err
				}

			}
		}
	}

	byteValue, _ := os.ReadFile(filepath)
	if err != nil {
		return nil, err
	}

	return byteValue, nil
}

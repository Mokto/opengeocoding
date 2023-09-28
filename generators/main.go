package main

import (
	"fmt"
	"generators/scripts"
)

func main() {
	// fmt.Println("Generating countries...")
	// scripts.GenerateCountriesLabels()
	// fmt.Println("Generating cities...")
	// scripts.GenerateCitiesLabels()
	fmt.Println("Generating countries languages...")
	scripts.GenerateCountriesLanguages()
}

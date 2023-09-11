package config

import (
	"os"
	"strconv"
)

// Simple helper function to read an environment or return a default value
func GetEnv(key string, defaultVal string) string {
	if value, exists := os.LookupEnv(key); exists {
		return value
	}

	return defaultVal
}

// Simple helper function to read an environment variable into integer or return a default value
func GetEnvAsInt(name string, defaultVal int) int {
	valueStr := GetEnv(name, "")
	if value, err := strconv.Atoi(valueStr); err == nil {
		return value
	}

	return defaultVal
}

// Helper to read an environment variable into a bool or return default value
func GetEnvAsBool(name string, defaultVal bool) bool {
	valStr := GetEnv(name, "")
	if val, err := strconv.ParseBool(valStr); err == nil {
		return val
	}

	return defaultVal
}

// Simple helper function to read an environment variable into integer or return a default value
func GetEnvAsFloat64(name string, defaultVal float64) float64 {
	valueStr := GetEnv(name, "")
	if s, err := strconv.ParseFloat(valueStr, 64); err == nil {
		return s
	}
	return defaultVal
}

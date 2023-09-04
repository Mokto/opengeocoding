package geolabels

import (
	"testing"
)

func TestDenmark(t *testing.T) {
	country_code := GetCountryCodeFromLabel("Denmark")
	want := "dk"
	if country_code != want {
		t.Fatalf(`GetCountryCodeFromLabel("Denmark") = %q, want match for %#q`, country_code, want)
	}
}

func TestDanmark(t *testing.T) {
	country_code := GetCountryCodeFromLabel("Danmark")
	want := "dk"
	if country_code != want {
		t.Fatalf(`GetCountryCodeFromLabel("Danmark") = %q, want match for %#q`, country_code, want)
	}
}

func TestFrance(t *testing.T) {
	country_code := GetCountryCodeFromLabel("France")
	want := "fr"
	if country_code != want {
		t.Fatalf(`GetCountryCodeFromLabel("France") = %q, want match for %#q`, country_code, want)
	}
}

func TestFranceCaseInsensitive(t *testing.T) {
	country_code := GetCountryCodeFromLabel("france")
	want := "fr"
	if country_code != want {
		t.Fatalf(`GetCountryCodeFromLabel("france") = %q, want match for %#q`, country_code, want)
	}
}

func TestRandom(t *testing.T) {
	country_code := GetCountryCodeFromLabel("NonExisting")
	want := ""
	if country_code != want {
		t.Fatalf(`GetCountryCodeFromLabel("NonExisting") = %q, want match for %#q`, country_code, want)
	}
}

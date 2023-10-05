package forward_test

import (
	"bytes"
	"encoding/json"
	"geocoding/pkg/container"
	"geocoding/pkg/forward"
	"geocoding/pkg/proto"
	"math"
	"testing"

	"github.com/bradleyjkemp/cupaloy"
	_ "github.com/go-sql-driver/mysql"
	"google.golang.org/protobuf/encoding/protojson"
)

func TestForwardFull(t *testing.T) {
	container := container.Init()

	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 th 2300 Copenhagen S",
		"Geislersgade 14, 3 th 2300 Copenhagen",
		"Geislersgade 14, 3th 2300 Copenhagen",
		"461 W Main St, Cheshire, 06410",
		"The Book Club 100-106 Leonard St Shoreditch London EC2A 4RH, United Kingdom",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
		"7926 Old Seward Hwy; Suite A6; Anchorage, Alaska 99518, US",
		"Tunggak Jati Regency Blok C1 No. 26 Tunggak Jati, Kec. Karawang Barat; Karawang, Jawa Barat 41351, ID, India",
		"146 Valero Street; The Pearlbank Centre; Makati, National Capital Region 1227, PH, Philippines",
		"Prospect House; Colliery Close; Chesterfield, Derbyshire S43 3QE, GB, United Kingdom",
		"Suite 4420, 17B Farnham Street; Parnell, Auckland 1052, NZ, New Zealand",
		"Rathsfelder Straße 6b; Nordhausen, Thuringia 99734, DE",
		"Hillerodgade",
		"178 Columbus Avenue; #231573; New York, NY 10023, US, United States",
		"178 Columbus Avenue; #231573; New York, NY 10023, United States",
		"5840 Autoport Mall; San Diego, California 92121, us",
		"Calle Montes Urales Norte; Lomas de Chapultepec, Distrito Federal 11000, Mexico",
		"22 Rue du Docteur Jean Michel; VUILLECIN, FR, France",
		"Shangjiangcheng Industrial Zone; Dongguan 523000, China",
		"Delstrup; Münster, de",
		"Kryssarvägen 4; Täby, SE",
		"485 Madison Avenue; 10th Floor; New York, NY 10022, US",
		"Kalvebod Brygge 1-3; København Denmark",
	}

	for _, query := range queries {
		location, err := forward.Forward(container, query)
		if err != nil {
			panic(err)
		}
		goldenFile(t, query, location)
	}

}
func TestForwardCities(t *testing.T) {
	container := container.Init()

	queries := []string{
		"Lawrence, Kansas, United States",
		"Vicenza, Veneto, Italy",
		"Roma, Rm, Italy",
		"Herne Bay, United Kingdom",
		"Kansas City, Missouri, united states",
		"Metz",
		"London",
		"Arnhem, NL, Netherlands",
		"CDMX, CDMX, MX, Mexico",
		"Wellington, Florida 33614, US, United States",
		"Stevensville, Michigan, United States",
	}

	for _, query := range queries {
		location, err := forward.Forward(container, query)
		if err != nil {
			panic(err)
		}
		goldenFile(t, query, location)
	}

}

func goldenFile(t *testing.T, name string, location *proto.ForwardResult) {
	if location.Location != nil {
		location.Location.Lat = float32(math.Floor(float64(location.Location.Lat)*100) / 100)
		location.Location.Long = float32(math.Floor(float64(location.Location.Long)*100) / 100)
	}
	locationStr, err := protojson.Marshal(location)
	if err != nil {
		panic(err)
	}

	t.Run(name, func(t *testing.T) {
		var prettyJSON bytes.Buffer
		if err := json.Indent(&prettyJSON, []byte(locationStr), "", "    "); err != nil {
			panic(err)
		}
		cupaloy.SnapshotT(t, prettyJSON.String())
	})
}

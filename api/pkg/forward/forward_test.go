package forward

import (
	"encoding/json"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/proto"
	"testing"

	"github.com/bradleyjkemp/cupaloy"
	_ "github.com/go-sql-driver/mysql"
)

func TestForward(t *testing.T) {
	database := manticoresearch.InitDatabase()

	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 th 2300 Copenhagen S",
		"Geislersgade 14, 3 th 2300 Copenhagen",
		"Geislersgade 14, 3th 2300 Copenhagen",
		"461 W Main St, Cheshire, 06410",
		"The Book Club 100-106 Leonard St Shoreditch London EC2A 4RH, United Kingdom",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
		"7926 Old Seward Hwy; Suite A6; Anchorage, Alaska 99518, US",
		"Lawrence, Kansas, United States",
		"Tunggak Jati Regency Blok C1 No. 26 Tunggak Jati, Kec. Karawang Barat; Karawang, Jawa Barat 41351, ID, India",
		"146 Valero Street; The Pearlbank Centre; Makati, National Capital Region 1227, PH, Philippines",
		"Prospect House; Colliery Close; Chesterfield, Derbyshire S43 3QE, GB, United Kingdom",
		"Vicenza, Veneto, Italy",
		"Suite 4420, 17B Farnham Street; Parnell, Auckland 1052, NZ, New Zealand",
		"Rathsfelder Straße 6b; Nordhausen, Thuringia 99734, DE",
		"Roma, Rm, Italy",
		"Herne Bay, United Kingdom",
		"Kansas City, Missouri, united states",
		"Hillerodgade",
		"Metz",
		"London",
		"178 Columbus Avenue; #231573; New York, NY 10023, US, United States",
		"178 Columbus Avenue; #231573; New York, NY 10023, United States",
		"5840 Autoport Mall; San Diego, California 92121, us",
		"Arnhem, NL, Netherlands",
		"Calle Montes Urales Norte; Lomas de Chapultepec, Distrito Federal 11000, Mexico",
		"22 Rue du Docteur Jean Michel; VUILLECIN, FR, France",
		"Shangjiangcheng Industrial Zone; Dongguan 523000, China",
		"CDMX, CDMX, MX, Mexico",
	}

	for _, query := range queries {
		location, err := Forward(database, query)
		if err != nil {
			panic(err)
		}
		goldenFile(t, query, location)
	}

}

func goldenFile(t *testing.T, name string, location *proto.ForwardResult) {
	locationStr, err := json.MarshalIndent(location, "", "  ")
	if err != nil {
		panic(err)
	}

	t.Run(name, func(t *testing.T) {
		cupaloy.SnapshotT(t, locationStr)
	})
}

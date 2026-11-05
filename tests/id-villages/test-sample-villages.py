import importlib.util
import io
import unittest
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("villages", Path(__file__).with_name("sample-villages.py"))
villages = importlib.util.module_from_spec(spec)
spec.loader.exec_module(villages)

V = "http://www.wikidata.org/entity/"


def row(qid, code, label, kec="K1", point="Point(110.0 -7.0)", kind="Q26211545"):
    return {"item": {"value": V + qid}, "code": {"value": code}, "label": {"value": label},
            "kec": {"value": V + kec}, "point": {"value": point}, "type": {"value": V + kind}}


def workbook(rows):
    """A one-sheet xlsx with inline strings, as the COD-AB workbook's idn_admin4 sheet."""
    def cell(ref, value):
        if isinstance(value, str):
            return f'<c r="{ref}" t="inlineStr"><is><t>{value}</t></is></c>'
        return f'<c r="{ref}"><v>{value}</v></c>'
    sheet = "".join(
        f'<row r="{i}">' + "".join(cell(f"{chr(65 + j)}{i}", v) for j, v in enumerate(r)) + "</row>"
        for i, r in enumerate(rows, 1)
    )
    m = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"'
    r = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w") as z:
        z.writestr("xl/sharedStrings.xml", f"<sst {m}/>")
        z.writestr("xl/workbook.xml", f'<workbook {m} {r}><sheets><sheet name="idn_admin4" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/worksheets/sheet1.xml", f"<worksheet {m}><sheetData>{sheet}</sheetData></worksheet>")
    out.seek(0)
    return out


class Villages(unittest.TestCase):
    def test_reads_the_village_sheet(self):
        book = workbook([["adm4_pcode", "adm4_name", "center_lat", "center_lon", "area_sqkm"],
                         ["ID3315032001", "Pepe", -7.02, 110.65, 3.14159]])
        self.assertEqual(villages.bps_villages(book), {"ID3315032001": ("Pepe", -7.02, 110.65, 3.14159)})

    def test_written_as_addresses_write_them(self):
        v = {"kind": "Desa", "name": "Pepe", "kec": "Tegowanu", "kab": "Kabupaten Grobogan", "prov": "Jawa Tengah", "code": "3315032001"}
        self.assertEqual(villages.queries(v, True), [
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah, Indonesia",
            "Pepe, Tegowanu, Grobogan, Jawa Tengah, Indonesia",
            "Pepe, Grobogan, Indonesia",
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng",
        ])
        v = dict(v, kind="Kelurahan", kab="Kota Jambi", prov="Jambi", code="1571011001")
        self.assertEqual(villages.queries(v, False)[-1], "Kelurahan Pepe, Kec. Tegowanu, Kota Jambi, Jambi")

    def test_one_chain_and_the_kind_from_the_type(self):
        chains = {V + "K1": {("Tegowanu", "Grobogan", "Jawa Tengah")}, V + "K2": {("A", "B", "C"), ("A", "D", "C")}}
        found = villages.read_villages({
            "Q1": [row("Q1", "3315032001", "Pepe"), row("Q1", "3315032001", "Pepe", kind="Q965568")],
            "Q2": [row("Q2", "3315032002", "Two", kec="K2")],
        }, chains)
        self.assertEqual([(v["qid"], v["kind"], v["kab"]) for v in found], [("Q1", "Kelurahan", "Grobogan")])

    def test_vetted_against_bps(self):
        v = lambda qid, code, name, lat=-7.0: {"qid": qid, "code": code, "name": name, "kec": "K", "kab": "B", "lat": lat, "lon": 110.0}
        bps = {"ID3315032001": ("Pepe", -7.0, 110.0, 1.0), "ID3315032002": ("Other", -7.0, 110.0, 1.0),
               "ID3315032003": ("Far", -7.0, 110.0, 1.0), "ID3315042001": ("Twin", -7.0, 110.0, 1.0),
               "ID3315042002": ("Twin", -7.0, 110.0, 1.0)}
        eligible, rejected = villages.vet([
            v("Q1", "3315032001", "Pepe"), v("Q2", "3315032002", "Renamed"), v("Q3", "3315032003", "Far", lat=-7.1),
            v("Q4", "3315042001", "Twin"), v("Q5", "3315042002", "Twin"), v("Q6", "3315032009", "Missing"),
        ], bps)
        self.assertEqual([(e["qid"], e["radius_km"]) for e in eligible], [("Q1", 2.0)])
        self.assertEqual(rejected, {"no_bps_village": 1, "bps_name_differs": 1, "far_from_bps": 1, "ambiguous": 2, "odd_label": 0})

    def test_cities_are_numbered_from_71(self):
        self.assertEqual(villages.regency("Kota Semarang", "3374010001"), ("Kota Semarang", "Kota Semarang"))
        self.assertEqual(villages.regency("Grobogan", "3315032001"), ("Kab. Grobogan", "Grobogan"))


if __name__ == "__main__":
    unittest.main()

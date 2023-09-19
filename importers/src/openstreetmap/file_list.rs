use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OsmCountryFiles {
    pub url: String,
    pub country_codes: Vec<String>,
}

pub fn get_osm_country_files() -> Vec<OsmCountryFiles> {
    let data = r#"[{"url": "https://download.geofabrik.de/europe/sweden-latest.osm.pbf", "country_codes": []}]"#;
    return serde_json::from_str(data).unwrap();
    // https://download.geofabrik.de/index-v1.json
    // JSON.stringify(a.features.map(f => ({url: f.properties.urls.pbf,"country_codes": f.properties["iso3166-1:alpha2"]?.map(a => a.toLowerCase())})).filter(a => a.url.split("/").length - 1==4 && !!a.country_codes))
    let data = r#"[{"url":"https://download.geofabrik.de/asia/afghanistan-latest.osm.pbf","country_codes":["af"]},{"url":"https://download.geofabrik.de/europe/albania-latest.osm.pbf","country_codes":["al"]},{"url":"https://download.geofabrik.de/africa/algeria-latest.osm.pbf","country_codes":["dz"]},{"url":"https://download.geofabrik.de/australia-oceania/american-oceania-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/europe/andorra-latest.osm.pbf","country_codes":["ad"]},{"url":"https://download.geofabrik.de/africa/angola-latest.osm.pbf","country_codes":["ao"]},{"url":"https://download.geofabrik.de/south-america/argentina-latest.osm.pbf","country_codes":["ar"]},{"url":"https://download.geofabrik.de/asia/armenia-latest.osm.pbf","country_codes":["am"]},{"url":"https://download.geofabrik.de/australia-oceania/australia-latest.osm.pbf","country_codes":["au"]},{"url":"https://download.geofabrik.de/europe/austria-latest.osm.pbf","country_codes":["at"]},{"url":"https://download.geofabrik.de/asia/azerbaijan-latest.osm.pbf","country_codes":["az"]},{"url":"https://download.geofabrik.de/asia/bangladesh-latest.osm.pbf","country_codes":["bd"]},{"url":"https://download.geofabrik.de/europe/belarus-latest.osm.pbf","country_codes":["by"]},{"url":"https://download.geofabrik.de/europe/belgium-latest.osm.pbf","country_codes":["be"]},{"url":"https://download.geofabrik.de/central-america/belize-latest.osm.pbf","country_codes":["bz"]},{"url":"https://download.geofabrik.de/africa/benin-latest.osm.pbf","country_codes":["bj"]},{"url":"https://download.geofabrik.de/asia/bhutan-latest.osm.pbf","country_codes":["bt"]},{"url":"https://download.geofabrik.de/south-america/bolivia-latest.osm.pbf","country_codes":["bo"]},{"url":"https://download.geofabrik.de/europe/bosnia-herzegovina-latest.osm.pbf","country_codes":["ba"]},{"url":"https://download.geofabrik.de/africa/botswana-latest.osm.pbf","country_codes":["bw"]},{"url":"https://download.geofabrik.de/south-america/brazil-latest.osm.pbf","country_codes":["br"]},{"url":"https://download.geofabrik.de/europe/bulgaria-latest.osm.pbf","country_codes":["bg"]},{"url":"https://download.geofabrik.de/africa/burkina-faso-latest.osm.pbf","country_codes":["bf"]},{"url":"https://download.geofabrik.de/africa/burundi-latest.osm.pbf","country_codes":["bi"]},{"url":"https://download.geofabrik.de/asia/cambodia-latest.osm.pbf","country_codes":["kh"]},{"url":"https://download.geofabrik.de/africa/cameroon-latest.osm.pbf","country_codes":["cm"]},{"url":"https://download.geofabrik.de/north-america/canada-latest.osm.pbf","country_codes":["ca"]},{"url":"https://download.geofabrik.de/africa/cape-verde-latest.osm.pbf","country_codes":["cv"]},{"url":"https://download.geofabrik.de/africa/central-african-republic-latest.osm.pbf","country_codes":["cf"]},{"url":"https://download.geofabrik.de/africa/chad-latest.osm.pbf","country_codes":["td"]},{"url":"https://download.geofabrik.de/south-america/chile-latest.osm.pbf","country_codes":["cl"]},{"url":"https://download.geofabrik.de/asia/china-latest.osm.pbf","country_codes":["cn"]},{"url":"https://download.geofabrik.de/south-america/colombia-latest.osm.pbf","country_codes":["co"]},{"url":"https://download.geofabrik.de/africa/congo-brazzaville-latest.osm.pbf","country_codes":["cg"]},{"url":"https://download.geofabrik.de/africa/congo-democratic-republic-latest.osm.pbf","country_codes":["cd"]},{"url":"https://download.geofabrik.de/australia-oceania/cook-islands-latest.osm.pbf","country_codes":["ck"]},{"url":"https://download.geofabrik.de/central-america/costa-rica-latest.osm.pbf","country_codes":["cr"]},{"url":"https://download.geofabrik.de/europe/croatia-latest.osm.pbf","country_codes":["hr"]},{"url":"https://download.geofabrik.de/europe/cyprus-latest.osm.pbf","country_codes":["cy"]},{"url":"https://download.geofabrik.de/europe/czech-republic-latest.osm.pbf","country_codes":["cz"]},{"url":"https://download.geofabrik.de/europe/denmark-latest.osm.pbf","country_codes":["dk"]},{"url":"https://download.geofabrik.de/africa/djibouti-latest.osm.pbf","country_codes":["dj"]},{"url":"https://download.geofabrik.de/asia/east-timor-latest.osm.pbf","country_codes":["tl"]},{"url":"https://download.geofabrik.de/south-america/ecuador-latest.osm.pbf","country_codes":["ec"]},{"url":"https://download.geofabrik.de/africa/egypt-latest.osm.pbf","country_codes":["eg"]},{"url":"https://download.geofabrik.de/central-america/el-salvador-latest.osm.pbf","country_codes":["sv"]},{"url":"https://download.geofabrik.de/africa/equatorial-guinea-latest.osm.pbf","country_codes":["gq"]},{"url":"https://download.geofabrik.de/africa/eritrea-latest.osm.pbf","country_codes":["er"]},{"url":"https://download.geofabrik.de/europe/estonia-latest.osm.pbf","country_codes":["ee"]},{"url":"https://download.geofabrik.de/africa/ethiopia-latest.osm.pbf","country_codes":["et"]},{"url":"https://download.geofabrik.de/europe/faroe-islands-latest.osm.pbf","country_codes":["fo"]},{"url":"https://download.geofabrik.de/australia-oceania/fiji-latest.osm.pbf","country_codes":["fj"]},{"url":"https://download.geofabrik.de/europe/finland-latest.osm.pbf","country_codes":["fi"]},{"url":"https://download.geofabrik.de/europe/france-latest.osm.pbf","country_codes":["fr"]},{"url":"https://download.geofabrik.de/africa/gabon-latest.osm.pbf","country_codes":["ga"]},{"url":"https://download.geofabrik.de/asia/gcc-states-latest.osm.pbf","country_codes":["qa","ae","om","bh","kw"]},{"url":"https://download.geofabrik.de/europe/georgia-latest.osm.pbf","country_codes":["ge"]},{"url":"https://download.geofabrik.de/europe/germany-latest.osm.pbf","country_codes":["de"]},{"url":"https://download.geofabrik.de/africa/ghana-latest.osm.pbf","country_codes":["gh"]},{"url":"https://download.geofabrik.de/europe/great-britain-latest.osm.pbf","country_codes":["gb"]},{"url":"https://download.geofabrik.de/europe/greece-latest.osm.pbf","country_codes":["gr"]},{"url":"https://download.geofabrik.de/north-america/greenland-latest.osm.pbf","country_codes":["gl"]},{"url":"https://download.geofabrik.de/central-america/guatemala-latest.osm.pbf","country_codes":["gt"]},{"url":"https://download.geofabrik.de/africa/guinea-latest.osm.pbf","country_codes":["gn"]},{"url":"https://download.geofabrik.de/africa/guinea-bissau-latest.osm.pbf","country_codes":["gw"]},{"url":"https://download.geofabrik.de/south-america/guyana-latest.osm.pbf","country_codes":["gy"]},{"url":"https://download.geofabrik.de/central-america/honduras-latest.osm.pbf","country_codes":["hn"]},{"url":"https://download.geofabrik.de/europe/hungary-latest.osm.pbf","country_codes":["hu"]},{"url":"https://download.geofabrik.de/europe/iceland-latest.osm.pbf","country_codes":["is"]},{"url":"https://download.geofabrik.de/australia-oceania/ile-de-clipperton-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/asia/india-latest.osm.pbf","country_codes":["in"]},{"url":"https://download.geofabrik.de/asia/indonesia-latest.osm.pbf","country_codes":["id"]},{"url":"https://download.geofabrik.de/asia/iran-latest.osm.pbf","country_codes":["ir"]},{"url":"https://download.geofabrik.de/asia/iraq-latest.osm.pbf","country_codes":["iq"]},{"url":"https://download.geofabrik.de/europe/ireland-and-northern-ireland-latest.osm.pbf","country_codes":["ie"]},{"url":"https://download.geofabrik.de/asia/israel-and-palestine-latest.osm.pbf","country_codes":["ps","il"]},{"url":"https://download.geofabrik.de/europe/italy-latest.osm.pbf","country_codes":["it"]},{"url":"https://download.geofabrik.de/africa/ivory-coast-latest.osm.pbf","country_codes":["ci"]},{"url":"https://download.geofabrik.de/asia/japan-latest.osm.pbf","country_codes":["jp"]},{"url":"https://download.geofabrik.de/asia/jordan-latest.osm.pbf","country_codes":["jo"]},{"url":"https://download.geofabrik.de/asia/kazakhstan-latest.osm.pbf","country_codes":["kz"]},{"url":"https://download.geofabrik.de/africa/kenya-latest.osm.pbf","country_codes":["ke"]},{"url":"https://download.geofabrik.de/australia-oceania/kiribati-latest.osm.pbf","country_codes":["ki"]},{"url":"https://download.geofabrik.de/asia/kyrgyzstan-latest.osm.pbf","country_codes":["kg"]},{"url":"https://download.geofabrik.de/asia/laos-latest.osm.pbf","country_codes":["la"]},{"url":"https://download.geofabrik.de/europe/latvia-latest.osm.pbf","country_codes":["lv"]},{"url":"https://download.geofabrik.de/asia/lebanon-latest.osm.pbf","country_codes":["lb"]},{"url":"https://download.geofabrik.de/africa/lesotho-latest.osm.pbf","country_codes":["ls"]},{"url":"https://download.geofabrik.de/africa/liberia-latest.osm.pbf","country_codes":["lr"]},{"url":"https://download.geofabrik.de/africa/libya-latest.osm.pbf","country_codes":["ly"]},{"url":"https://download.geofabrik.de/europe/liechtenstein-latest.osm.pbf","country_codes":["li"]},{"url":"https://download.geofabrik.de/europe/lithuania-latest.osm.pbf","country_codes":["lt"]},{"url":"https://download.geofabrik.de/europe/luxembourg-latest.osm.pbf","country_codes":["lu"]},{"url":"https://download.geofabrik.de/europe/macedonia-latest.osm.pbf","country_codes":["mk"]},{"url":"https://download.geofabrik.de/africa/madagascar-latest.osm.pbf","country_codes":["mg"]},{"url":"https://download.geofabrik.de/africa/malawi-latest.osm.pbf","country_codes":["mw"]},{"url":"https://download.geofabrik.de/asia/malaysia-singapore-brunei-latest.osm.pbf","country_codes":["my"]},{"url":"https://download.geofabrik.de/asia/maldives-latest.osm.pbf","country_codes":["mv"]},{"url":"https://download.geofabrik.de/africa/mali-latest.osm.pbf","country_codes":["ml"]},{"url":"https://download.geofabrik.de/europe/malta-latest.osm.pbf","country_codes":["mt"]},{"url":"https://download.geofabrik.de/australia-oceania/marshall-islands-latest.osm.pbf","country_codes":["mh"]},{"url":"https://download.geofabrik.de/africa/mauritania-latest.osm.pbf","country_codes":["mr"]},{"url":"https://download.geofabrik.de/africa/mauritius-latest.osm.pbf","country_codes":["mu"]},{"url":"https://download.geofabrik.de/north-america/mexico-latest.osm.pbf","country_codes":["mx"]},{"url":"https://download.geofabrik.de/australia-oceania/micronesia-latest.osm.pbf","country_codes":["fm"]},{"url":"https://download.geofabrik.de/europe/moldova-latest.osm.pbf","country_codes":["md"]},{"url":"https://download.geofabrik.de/europe/monaco-latest.osm.pbf","country_codes":["mc"]},{"url":"https://download.geofabrik.de/asia/mongolia-latest.osm.pbf","country_codes":["mn"]},{"url":"https://download.geofabrik.de/europe/montenegro-latest.osm.pbf","country_codes":["me"]},{"url":"https://download.geofabrik.de/africa/morocco-latest.osm.pbf","country_codes":["ma"]},{"url":"https://download.geofabrik.de/africa/mozambique-latest.osm.pbf","country_codes":["mz"]},{"url":"https://download.geofabrik.de/asia/myanmar-latest.osm.pbf","country_codes":["mm"]},{"url":"https://download.geofabrik.de/africa/namibia-latest.osm.pbf","country_codes":["na"]},{"url":"https://download.geofabrik.de/australia-oceania/nauru-latest.osm.pbf","country_codes":["nr"]},{"url":"https://download.geofabrik.de/asia/nepal-latest.osm.pbf","country_codes":["np"]},{"url":"https://download.geofabrik.de/europe/netherlands-latest.osm.pbf","country_codes":["nl"]},{"url":"https://download.geofabrik.de/australia-oceania/new-caledonia-latest.osm.pbf","country_codes":["nc"]},{"url":"https://download.geofabrik.de/australia-oceania/new-zealand-latest.osm.pbf","country_codes":["nz"]},{"url":"https://download.geofabrik.de/central-america/nicaragua-latest.osm.pbf","country_codes":["ni"]},{"url":"https://download.geofabrik.de/africa/niger-latest.osm.pbf","country_codes":["ne"]},{"url":"https://download.geofabrik.de/africa/nigeria-latest.osm.pbf","country_codes":["ng"]},{"url":"https://download.geofabrik.de/australia-oceania/niue-latest.osm.pbf","country_codes":["nu"]},{"url":"https://download.geofabrik.de/asia/north-korea-latest.osm.pbf","country_codes":["kp"]},{"url":"https://download.geofabrik.de/europe/norway-latest.osm.pbf","country_codes":["no"]},{"url":"https://download.geofabrik.de/asia/pakistan-latest.osm.pbf","country_codes":["pk"]},{"url":"https://download.geofabrik.de/australia-oceania/palau-latest.osm.pbf","country_codes":["pw"]},{"url":"https://download.geofabrik.de/australia-oceania/papua-new-guinea-latest.osm.pbf","country_codes":["pg"]},{"url":"https://download.geofabrik.de/south-america/paraguay-latest.osm.pbf","country_codes":["py"]},{"url":"https://download.geofabrik.de/south-america/peru-latest.osm.pbf","country_codes":["pe"]},{"url":"https://download.geofabrik.de/asia/philippines-latest.osm.pbf","country_codes":["ph"]},{"url":"https://download.geofabrik.de/australia-oceania/pitcairn-islands-latest.osm.pbf","country_codes":["mh"]},{"url":"https://download.geofabrik.de/europe/poland-latest.osm.pbf","country_codes":["pl"]},{"url":"https://download.geofabrik.de/australia-oceania/polynesie-francaise-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/europe/portugal-latest.osm.pbf","country_codes":["pt"]},{"url":"https://download.geofabrik.de/europe/romania-latest.osm.pbf","country_codes":["ro"]},{"url":"https://download.geofabrik.de/africa/rwanda-latest.osm.pbf","country_codes":["rw"]},{"url":"https://download.geofabrik.de/africa/saint-helena-ascension-and-tristan-da-cunha-latest.osm.pbf","country_codes":["sh"]},{"url":"https://download.geofabrik.de/australia-oceania/samoa-latest.osm.pbf","country_codes":["ws"]},{"url":"https://download.geofabrik.de/africa/sao-tome-and-principe-latest.osm.pbf","country_codes":["st"]},{"url":"https://download.geofabrik.de/africa/senegal-and-gambia-latest.osm.pbf","country_codes":["sn","gm"]},{"url":"https://download.geofabrik.de/europe/serbia-latest.osm.pbf","country_codes":["rs"]},{"url":"https://download.geofabrik.de/africa/seychelles-latest.osm.pbf","country_codes":["sc"]},{"url":"https://download.geofabrik.de/africa/sierra-leone-latest.osm.pbf","country_codes":["sl"]},{"url":"https://download.geofabrik.de/europe/slovakia-latest.osm.pbf","country_codes":["sk"]},{"url":"https://download.geofabrik.de/europe/slovenia-latest.osm.pbf","country_codes":["si"]},{"url":"https://download.geofabrik.de/australia-oceania/solomon-islands-latest.osm.pbf","country_codes":["sb"]},{"url":"https://download.geofabrik.de/africa/somalia-latest.osm.pbf","country_codes":["so"]},{"url":"https://download.geofabrik.de/africa/south-africa-latest.osm.pbf","country_codes":["za"]},{"url":"https://download.geofabrik.de/asia/south-korea-latest.osm.pbf","country_codes":["kr"]},{"url":"https://download.geofabrik.de/africa/south-sudan-latest.osm.pbf","country_codes":["ss"]},{"url":"https://download.geofabrik.de/europe/spain-latest.osm.pbf","country_codes":["es"]},{"url":"https://download.geofabrik.de/asia/sri-lanka-latest.osm.pbf","country_codes":["lk"]},{"url":"https://download.geofabrik.de/africa/sudan-latest.osm.pbf","country_codes":["sd"]},{"url":"https://download.geofabrik.de/south-america/suriname-latest.osm.pbf","country_codes":["sr"]},{"url":"https://download.geofabrik.de/africa/swaziland-latest.osm.pbf","country_codes":["sz"]},{"url":"https://download.geofabrik.de/europe/sweden-latest.osm.pbf","country_codes":["se"]},{"url":"https://download.geofabrik.de/europe/switzerland-latest.osm.pbf","country_codes":["ch"]},{"url":"https://download.geofabrik.de/asia/syria-latest.osm.pbf","country_codes":["sy"]},{"url":"https://download.geofabrik.de/asia/taiwan-latest.osm.pbf","country_codes":["tw"]},{"url":"https://download.geofabrik.de/asia/tajikistan-latest.osm.pbf","country_codes":["tj"]},{"url":"https://download.geofabrik.de/africa/tanzania-latest.osm.pbf","country_codes":["tz"]},{"url":"https://download.geofabrik.de/asia/thailand-latest.osm.pbf","country_codes":["th"]},{"url":"https://download.geofabrik.de/africa/togo-latest.osm.pbf","country_codes":["tg"]},{"url":"https://download.geofabrik.de/australia-oceania/tokelau-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/australia-oceania/tonga-latest.osm.pbf","country_codes":["to"]},{"url":"https://download.geofabrik.de/africa/tunisia-latest.osm.pbf","country_codes":["tn"]},{"url":"https://download.geofabrik.de/europe/turkey-latest.osm.pbf","country_codes":["tr"]},{"url":"https://download.geofabrik.de/asia/turkmenistan-latest.osm.pbf","country_codes":["tm"]},{"url":"https://download.geofabrik.de/australia-oceania/tuvalu-latest.osm.pbf","country_codes":["tv"]},{"url":"https://download.geofabrik.de/africa/uganda-latest.osm.pbf","country_codes":["ug"]},{"url":"https://download.geofabrik.de/europe/ukraine-latest.osm.pbf","country_codes":["ua"]},{"url":"https://download.geofabrik.de/south-america/uruguay-latest.osm.pbf","country_codes":["uy"]},{"url":"https://download.geofabrik.de/north-america/us-latest.osm.pbf","country_codes":["us"]},{"url":"https://download.geofabrik.de/asia/uzbekistan-latest.osm.pbf","country_codes":["uz"]},{"url":"https://download.geofabrik.de/australia-oceania/vanuatu-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/south-america/venezuela-latest.osm.pbf","country_codes":["ve"]},{"url":"https://download.geofabrik.de/asia/vietnam-latest.osm.pbf","country_codes":["vn"]},{"url":"https://download.geofabrik.de/australia-oceania/wallis-et-futuna-latest.osm.pbf","country_codes":["vu"]},{"url":"https://download.geofabrik.de/asia/yemen-latest.osm.pbf","country_codes":["ye"]},{"url":"https://download.geofabrik.de/africa/zambia-latest.osm.pbf","country_codes":["zm"]},{"url":"https://download.geofabrik.de/africa/zimbabwe-latest.osm.pbf","country_codes":["zw"]}]"#;

    return serde_json::from_str(data).unwrap();
}

pub fn get_osm_country_files() -> Vec<&'static str> {
    // https://download.geofabrik.de/index-v1.json
    // return vec!["https://download.geofabrik.de/africa/algeria-latest.osm.pbf"];
    return vec![
        "https://download.geofabrik.de/asia/afghanistan-latest.osm.pbf","https://download.geofabrik.de/europe/albania-latest.osm.pbf","https://download.geofabrik.de/africa/algeria-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/american-oceania-latest.osm.pbf","https://download.geofabrik.de/europe/andorra-latest.osm.pbf","https://download.geofabrik.de/africa/angola-latest.osm.pbf","https://download.geofabrik.de/antarctica-latest.osm.pbf","https://download.geofabrik.de/south-america/argentina-latest.osm.pbf","https://download.geofabrik.de/asia/armenia-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/australia-latest.osm.pbf","https://download.geofabrik.de/europe/austria-latest.osm.pbf","https://download.geofabrik.de/asia/azerbaijan-latest.osm.pbf","https://download.geofabrik.de/asia/bangladesh-latest.osm.pbf","https://download.geofabrik.de/europe/belarus-latest.osm.pbf","https://download.geofabrik.de/europe/belgium-latest.osm.pbf","https://download.geofabrik.de/central-america/belize-latest.osm.pbf","https://download.geofabrik.de/africa/benin-latest.osm.pbf","https://download.geofabrik.de/asia/bhutan-latest.osm.pbf","https://download.geofabrik.de/south-america/bolivia-latest.osm.pbf","https://download.geofabrik.de/europe/bosnia-herzegovina-latest.osm.pbf","https://download.geofabrik.de/africa/botswana-latest.osm.pbf","https://download.geofabrik.de/south-america/brazil-latest.osm.pbf","https://download.geofabrik.de/europe/bulgaria-latest.osm.pbf","https://download.geofabrik.de/africa/burkina-faso-latest.osm.pbf","https://download.geofabrik.de/africa/burundi-latest.osm.pbf","https://download.geofabrik.de/asia/cambodia-latest.osm.pbf","https://download.geofabrik.de/africa/cameroon-latest.osm.pbf","https://download.geofabrik.de/north-america/canada-latest.osm.pbf","https://download.geofabrik.de/africa/cape-verde-latest.osm.pbf","https://download.geofabrik.de/africa/central-african-republic-latest.osm.pbf","https://download.geofabrik.de/africa/chad-latest.osm.pbf","https://download.geofabrik.de/south-america/chile-latest.osm.pbf","https://download.geofabrik.de/asia/china-latest.osm.pbf","https://download.geofabrik.de/south-america/colombia-latest.osm.pbf","https://download.geofabrik.de/africa/congo-brazzaville-latest.osm.pbf","https://download.geofabrik.de/africa/congo-democratic-republic-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/cook-islands-latest.osm.pbf","https://download.geofabrik.de/central-america/costa-rica-latest.osm.pbf","https://download.geofabrik.de/europe/croatia-latest.osm.pbf","https://download.geofabrik.de/europe/cyprus-latest.osm.pbf","https://download.geofabrik.de/europe/czech-republic-latest.osm.pbf","https://download.geofabrik.de/europe/denmark-latest.osm.pbf","https://download.geofabrik.de/africa/djibouti-latest.osm.pbf","https://download.geofabrik.de/asia/east-timor-latest.osm.pbf","https://download.geofabrik.de/south-america/ecuador-latest.osm.pbf","https://download.geofabrik.de/africa/egypt-latest.osm.pbf","https://download.geofabrik.de/central-america/el-salvador-latest.osm.pbf","https://download.geofabrik.de/africa/equatorial-guinea-latest.osm.pbf","https://download.geofabrik.de/africa/eritrea-latest.osm.pbf","https://download.geofabrik.de/europe/estonia-latest.osm.pbf","https://download.geofabrik.de/africa/ethiopia-latest.osm.pbf","https://download.geofabrik.de/europe/faroe-islands-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/fiji-latest.osm.pbf","https://download.geofabrik.de/europe/finland-latest.osm.pbf","https://download.geofabrik.de/europe/france-latest.osm.pbf","https://download.geofabrik.de/africa/gabon-latest.osm.pbf","https://download.geofabrik.de/asia/gcc-states-latest.osm.pbf","https://download.geofabrik.de/europe/georgia-latest.osm.pbf","https://download.geofabrik.de/europe/germany-latest.osm.pbf","https://download.geofabrik.de/africa/ghana-latest.osm.pbf","https://download.geofabrik.de/europe/great-britain-latest.osm.pbf","https://download.geofabrik.de/europe/greece-latest.osm.pbf","https://download.geofabrik.de/north-america/greenland-latest.osm.pbf","https://download.geofabrik.de/central-america/guatemala-latest.osm.pbf","https://download.geofabrik.de/africa/guinea-latest.osm.pbf","https://download.geofabrik.de/africa/guinea-bissau-latest.osm.pbf","https://download.geofabrik.de/south-america/guyana-latest.osm.pbf","https://download.geofabrik.de/europe/france/guyane-latest.osm.pbf","https://download.geofabrik.de/central-america/honduras-latest.osm.pbf","https://download.geofabrik.de/europe/hungary-latest.osm.pbf","https://download.geofabrik.de/europe/iceland-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/ile-de-clipperton-latest.osm.pbf","https://download.geofabrik.de/asia/india-latest.osm.pbf","https://download.geofabrik.de/asia/indonesia-latest.osm.pbf","https://download.geofabrik.de/asia/iran-latest.osm.pbf","https://download.geofabrik.de/asia/iraq-latest.osm.pbf","https://download.geofabrik.de/europe/ireland-and-northern-ireland-latest.osm.pbf","https://download.geofabrik.de/asia/israel-and-palestine-latest.osm.pbf","https://download.geofabrik.de/europe/italy-latest.osm.pbf","https://download.geofabrik.de/africa/ivory-coast-latest.osm.pbf","https://download.geofabrik.de/asia/japan-latest.osm.pbf","https://download.geofabrik.de/asia/jordan-latest.osm.pbf","https://download.geofabrik.de/asia/kazakhstan-latest.osm.pbf","https://download.geofabrik.de/africa/kenya-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/kiribati-latest.osm.pbf","https://download.geofabrik.de/asia/kyrgyzstan-latest.osm.pbf","https://download.geofabrik.de/asia/laos-latest.osm.pbf","https://download.geofabrik.de/europe/latvia-latest.osm.pbf","https://download.geofabrik.de/asia/lebanon-latest.osm.pbf","https://download.geofabrik.de/africa/lesotho-latest.osm.pbf","https://download.geofabrik.de/africa/liberia-latest.osm.pbf","https://download.geofabrik.de/africa/libya-latest.osm.pbf","https://download.geofabrik.de/europe/liechtenstein-latest.osm.pbf","https://download.geofabrik.de/europe/lithuania-latest.osm.pbf","https://download.geofabrik.de/europe/luxembourg-latest.osm.pbf","https://download.geofabrik.de/europe/macedonia-latest.osm.pbf","https://download.geofabrik.de/africa/madagascar-latest.osm.pbf","https://download.geofabrik.de/africa/malawi-latest.osm.pbf","https://download.geofabrik.de/asia/malaysia-singapore-brunei-latest.osm.pbf","https://download.geofabrik.de/asia/maldives-latest.osm.pbf","https://download.geofabrik.de/africa/mali-latest.osm.pbf","https://download.geofabrik.de/europe/malta-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/marshall-islands-latest.osm.pbf","https://download.geofabrik.de/africa/mauritania-latest.osm.pbf","https://download.geofabrik.de/africa/mauritius-latest.osm.pbf","https://download.geofabrik.de/north-america/mexico-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/micronesia-latest.osm.pbf","https://download.geofabrik.de/europe/moldova-latest.osm.pbf","https://download.geofabrik.de/europe/monaco-latest.osm.pbf","https://download.geofabrik.de/asia/mongolia-latest.osm.pbf","https://download.geofabrik.de/europe/montenegro-latest.osm.pbf","https://download.geofabrik.de/africa/morocco-latest.osm.pbf","https://download.geofabrik.de/africa/mozambique-latest.osm.pbf","https://download.geofabrik.de/asia/myanmar-latest.osm.pbf","https://download.geofabrik.de/africa/namibia-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/nauru-latest.osm.pbf","https://download.geofabrik.de/asia/nepal-latest.osm.pbf","https://download.geofabrik.de/europe/netherlands-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/new-caledonia-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/new-zealand-latest.osm.pbf","https://download.geofabrik.de/central-america/nicaragua-latest.osm.pbf","https://download.geofabrik.de/africa/niger-latest.osm.pbf","https://download.geofabrik.de/africa/nigeria-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/niue-latest.osm.pbf","https://download.geofabrik.de/asia/north-korea-latest.osm.pbf","https://download.geofabrik.de/europe/norway-latest.osm.pbf","https://download.geofabrik.de/asia/pakistan-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/palau-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/papua-new-guinea-latest.osm.pbf","https://download.geofabrik.de/south-america/paraguay-latest.osm.pbf","https://download.geofabrik.de/south-america/peru-latest.osm.pbf","https://download.geofabrik.de/asia/philippines-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/pitcairn-islands-latest.osm.pbf","https://download.geofabrik.de/europe/poland-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/polynesie-francaise-latest.osm.pbf","https://download.geofabrik.de/europe/portugal-latest.osm.pbf","https://download.geofabrik.de/europe/romania-latest.osm.pbf","https://download.geofabrik.de/russia-latest.osm.pbf","https://download.geofabrik.de/africa/rwanda-latest.osm.pbf","https://download.geofabrik.de/africa/saint-helena-ascension-and-tristan-da-cunha-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/samoa-latest.osm.pbf","https://download.geofabrik.de/africa/sao-tome-and-principe-latest.osm.pbf","https://download.geofabrik.de/africa/senegal-and-gambia-latest.osm.pbf","https://download.geofabrik.de/europe/serbia-latest.osm.pbf","https://download.geofabrik.de/africa/seychelles-latest.osm.pbf","https://download.geofabrik.de/africa/sierra-leone-latest.osm.pbf","https://download.geofabrik.de/europe/slovakia-latest.osm.pbf","https://download.geofabrik.de/europe/slovenia-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/solomon-islands-latest.osm.pbf","https://download.geofabrik.de/africa/somalia-latest.osm.pbf","https://download.geofabrik.de/africa/south-africa-latest.osm.pbf","https://download.geofabrik.de/asia/south-korea-latest.osm.pbf","https://download.geofabrik.de/africa/south-sudan-latest.osm.pbf","https://download.geofabrik.de/europe/spain-latest.osm.pbf","https://download.geofabrik.de/asia/sri-lanka-latest.osm.pbf","https://download.geofabrik.de/africa/sudan-latest.osm.pbf","https://download.geofabrik.de/south-america/suriname-latest.osm.pbf","https://download.geofabrik.de/africa/swaziland-latest.osm.pbf","https://download.geofabrik.de/europe/sweden-latest.osm.pbf","https://download.geofabrik.de/europe/switzerland-latest.osm.pbf","https://download.geofabrik.de/asia/syria-latest.osm.pbf","https://download.geofabrik.de/asia/taiwan-latest.osm.pbf","https://download.geofabrik.de/asia/tajikistan-latest.osm.pbf","https://download.geofabrik.de/africa/tanzania-latest.osm.pbf","https://download.geofabrik.de/asia/thailand-latest.osm.pbf","https://download.geofabrik.de/africa/togo-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/tokelau-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/tonga-latest.osm.pbf","https://download.geofabrik.de/africa/tunisia-latest.osm.pbf","https://download.geofabrik.de/europe/turkey-latest.osm.pbf","https://download.geofabrik.de/asia/turkmenistan-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/tuvalu-latest.osm.pbf","https://download.geofabrik.de/africa/uganda-latest.osm.pbf","https://download.geofabrik.de/europe/ukraine-latest.osm.pbf","https://download.geofabrik.de/south-america/uruguay-latest.osm.pbf","https://download.geofabrik.de/north-america/us-latest.osm.pbf","https://download.geofabrik.de/north-america/us/puerto-rico-latest.osm.pbf","https://download.geofabrik.de/north-america/us/us-virgin-islands-latest.osm.pbf","https://download.geofabrik.de/asia/uzbekistan-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/vanuatu-latest.osm.pbf","https://download.geofabrik.de/south-america/venezuela-latest.osm.pbf","https://download.geofabrik.de/asia/vietnam-latest.osm.pbf","https://download.geofabrik.de/australia-oceania/wallis-et-futuna-latest.osm.pbf","https://download.geofabrik.de/asia/yemen-latest.osm.pbf","https://download.geofabrik.de/africa/zambia-latest.osm.pbf","https://download.geofabrik.de/africa/zimbabwe-latest.osm.pbf"
    ];
}

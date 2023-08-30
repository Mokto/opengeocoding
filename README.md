CREATE TABLE IF NOT EXISTS openaddresses(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float)
ALTER CLUSTER manticore_cluster ADD openaddresses # until it succeeds


SELECT * FROM openaddresses WHERE MATCH('"Geislersgade 14, 3mf"/0.6') limit 1


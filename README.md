CREATE TABLE IF NOT EXISTS openaddresses(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'

ALTER CLUSTER manticore_cluster ADD openaddresses # until it succeeds

# Denmark — source access blocked

No new Danish registry corpus is included yet. National coverage must not be inferred from the existing Copenhagen City Hall suggestion examples.

The intended source is Danmarks Adresseregister (DAR), maintained by the municipalities and distributed by Klimadatastyrelsen. Its [CC BY 4.0 reuse terms](https://datafordeler.dk/vejledning/brugervilkaar/danmarks-adresseregister-dar/) permit attributed reuse, but access is separate from licensing.

On 2026-10-19 both the api.dataforsyningen.dk and dawa.aws.dk entrance-address endpoints returned HTTP 410. DAWA closed on 2026-10-01. The replacement [DAR file downloads](https://datafordeler.dk/dataoversigt/danmarks-adresseregister-dar/dar-fildownload) and modernized WFS require an account/IT-system registration and API key or OAuth. No accessible anonymous official full-address snapshot was verified. Do not substitute a commercial geocoder, fabricate floors/doors or commit credentials.

Copenhagen's anonymous municipal WFS exposes k101:dar_adgangsadresser at https://wfs-kbhkort.kk.dk/k101/ows with 85,364 entrance features at the time of the probe. It has DAR house-number UUIDs and entrance labels/points, but no floor/door fields, no verified national/supplementary-locality coverage and no positional-quality guarantee. It is not a replacement for the planned national full-address corpus.

To unblock, supply a licensed dated DAR export outside git with the linked current Adresse, Husnummer, Adressepunkt, NavngivenVej, Postnummer and supplementary-locality facts, source metadata, CRS and quality fields. Preserve original UUIDs, status and validity intervals; select current records at one documented time. Units must group by the entrance/building UUID for held-out selection and use the linked entrance point, not invented apartment coordinates. Test house letters, floors/doors, supplementary locality names and postal districts only where the captured source actually supplies them. Keep source acquisition separate from resolver/import fixes.

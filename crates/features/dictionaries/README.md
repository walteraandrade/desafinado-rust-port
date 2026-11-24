# Dictionaries

`pt_BR.aff.gz` and `pt_BR.dic.gz` are the Brazilian Portuguese Hunspell dictionary of the
LibreOffice VERO project (Raimundo Moura and contributors), gzipped, taken from
<https://github.com/LibreOffice/dictionaries/tree/master/pt_BR> on 2026-11-24.

The authors license it under the GNU LGPL version 3 and the Mozilla Public License; this app
uses it under the Mozilla Public License 2.0, the license `deny.toml` allows. The full notice is
in `pt_BR/README_pt_BR.txt` of that repository.

English needs no file here: Harper carries its own dictionary.

To update, download `pt_BR.aff` and `pt_BR.dic` from the link above and run
`gzip -9 -c pt_BR.aff > pt_BR.aff.gz` and `gzip -9 -c pt_BR.dic > pt_BR.dic.gz`.

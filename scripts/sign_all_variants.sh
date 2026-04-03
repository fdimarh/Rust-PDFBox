#!/bin/zsh
# sign_all_variants.sh — run every signing option combination and verify outputs
set -e

cd "$(dirname "$0")/.."

OUT="signed_outputs"
BIN="./target/debug/examples/digital_sign"
IN="tests/signing_assets/sample.pdf"
CERT="tests/signing_assets/ca-chain.pem"
KEY="tests/signing_assets/user-key.pem"

mkdir -p "$OUT"

PASS=0
FAIL=0
ERRORS=()

run() {
  local label="$1"; shift
  local out="$OUT/${label}.pdf"
  printf '\n════════════════════════════════════════════════════════\n'
  printf '▶  [%s]\n' "$label"
  printf '   Args: %s\n\n' "$*"
  if "$BIN" "$IN" -c "$CERT" -k "$KEY" -o "$out" "$@"; then
    PASS=$((PASS+1))
    printf '\n   ✅  Output: %s  (%d bytes)\n' "$out" "$(wc -c < "$out")"
  else
    FAIL=$((FAIL+1))
    ERRORS+=("$label")
    printf '\n   ❌  FAILED\n'
  fi
}

# ── PKCS7 variants (no live TSA needed — use --no-tsa) ────────────────────────

run "01_pkcs7_invisible" \
  -f pkcs7 --invisible --no-tsa \
  --reason "PKCS7 invisible (no timestamp)"

run "02_pkcs7_visible_rect" \
  -f pkcs7 --no-tsa \
  --rect "50,700,250,750" \
  --reason "PKCS7 visible rect" --name "Alice Signer" --contact "alice@example.com"

run "03_pkcs7_no_crl" \
  -f pkcs7 --invisible --no-tsa --no-crl \
  --reason "PKCS7 no CRL"

run "04_pkcs7_crl" \
  -f pkcs7 --invisible --no-tsa --crl \
  --reason "PKCS7 explicit CRL"

run "05_pkcs7_ocsp" \
  -f pkcs7 --invisible --no-tsa --ocsp --no-crl \
  --reason "PKCS7 OCSP only"

run "06_pkcs7_crl_ocsp" \
  -f pkcs7 --invisible --no-tsa --crl --ocsp \
  --reason "PKCS7 CRL+OCSP"

run "07_pkcs7_dss" \
  -f pkcs7 --invisible --no-tsa --dss \
  --reason "PKCS7 with DSS"

run "08_pkcs7_crl_dss" \
  -f pkcs7 --invisible --no-tsa --crl --dss \
  --reason "PKCS7 CRL+DSS"

run "09_pkcs7_crl_ocsp_dss" \
  -f pkcs7 --invisible --no-tsa --crl --ocsp --dss \
  --reason "PKCS7 CRL+OCSP+DSS"

run "10_pkcs7_custom_field" \
  -f pkcs7 --invisible --no-tsa \
  --field "MyCustomSig" \
  --reason "PKCS7 custom field name"

run "11_pkcs7_full_metadata" \
  -f pkcs7 --invisible --no-tsa \
  --name "Alice Dupont" \
  --contact "alice@corp.example" \
  --location "Paris, France" \
  --reason "PKCS7 full metadata"

run "12_pkcs7_visible_page1_metadata" \
  -f pkcs7 --no-tsa -p 1 \
  --rect "30,680,280,740" \
  --name "Charlie Signer" \
  --contact "charlie@example.com" \
  --location "Tokyo" \
  --reason "PKCS7 visible page-1 all metadata"

run "13_pkcs7_reserved_16k" \
  -f pkcs7 --invisible --no-tsa \
  --reserved 16384 \
  --reason "PKCS7 reserved 16k"

run "14_pkcs7_reserved_64k" \
  -f pkcs7 --invisible --no-tsa \
  --reserved 65536 \
  --reason "PKCS7 reserved 64k"

# ── PAdES B-B variants (no TSA — baseline level) ─────────────────────────────

run "15_pades_bb_invisible" \
  -f pades -l b-b --invisible --no-tsa \
  --reason "PAdES B-B invisible"

run "16_pades_bb_visible_rect" \
  -f pades -l b-b --no-tsa \
  --rect "50,700,250,750" \
  --reason "PAdES B-B visible rect" --name "Bob PAdES"

run "17_pades_bb_no_crl" \
  -f pades -l b-b --invisible --no-tsa --no-crl \
  --reason "PAdES B-B no CRL"

run "18_pades_bb_crl" \
  -f pades -l b-b --invisible --no-tsa --crl \
  --reason "PAdES B-B with CRL"

run "19_pades_bb_dss" \
  -f pades -l b-b --invisible --no-tsa --dss \
  --reason "PAdES B-B with DSS"

run "20_pades_bb_custom_field" \
  -f pades -l b-b --invisible --no-tsa \
  --field "PAdESSig" \
  --reason "PAdES B-B custom field"

run "21_pades_bb_full_metadata" \
  -f pades -l b-b --invisible --no-tsa \
  --name "Bob Mueller" \
  --contact "bob@example.de" \
  --location "Berlin, Germany" \
  --reason "PAdES B-B full metadata"

run "22_pades_bb_reserved_48k" \
  -f pades -l b-b --invisible --no-tsa \
  --reserved 49152 \
  --reason "PAdES B-B reserved 48k"

# ── PAdES B-T (needs live TSA — skipped if offline, attempt anyway) ──────────

run "23_pades_bt_invisible" \
  -f pades -l b-t --invisible \
  --tsa "http://timestamp.digicert.com" \
  --reason "PAdES B-T invisible (DigiCert TSA)"

run "24_pades_bt_visible" \
  -f pades -l b-t \
  --tsa "http://timestamp.digicert.com" \
  --rect "50,700,250,750" \
  --reason "PAdES B-T visible" --name "TSA Signer"

# ── PAdES B-LT (TSA + DSS auto-enabled) ──────────────────────────────────────

run "25_pades_blt_invisible" \
  -f pades -l b-lt --invisible \
  --tsa "http://timestamp.digicert.com" \
  --reason "PAdES B-LT invisible"

run "26_pades_blt_visible" \
  -f pades -l b-lt \
  --tsa "http://timestamp.digicert.com" \
  --rect "50,700,250,750" \
  --reason "PAdES B-LT visible" --name "LT Signer"

# ── PAdES B-LTA (TSA + DSS + doc timestamp) ──────────────────────────────────

run "27_pades_blta_invisible" \
  -f pades -l b-lta --invisible \
  --tsa "http://timestamp.digicert.com" \
  --reason "PAdES B-LTA invisible"

# ════════════════════════════════════════════════════════
# TSA-STAMPED VARIANTS — http://timestamp.digicert.com
# Every combination above, repeated with a live RFC 3161
# timestamp token embedded in the CMS unsigned attributes.
# ════════════════════════════════════════════════════════
TSA="http://timestamp.digicert.com"

# ── PKCS7 + TSA ──────────────────────────────────────────────────────────────

run "28_pkcs7_tsa_invisible" \
  -f pkcs7 --invisible \
  --tsa "$TSA" \
  --reason "PKCS7 + TSA invisible"

run "29_pkcs7_tsa_visible_rect" \
  -f pkcs7 --tsa "$TSA" \
  --rect "50,700,250,750" \
  --name "Alice TSA" --contact "alice@example.com" \
  --reason "PKCS7 + TSA visible rect"

run "30_pkcs7_tsa_no_crl" \
  -f pkcs7 --invisible --tsa "$TSA" --no-crl \
  --reason "PKCS7 + TSA no CRL"

run "31_pkcs7_tsa_crl" \
  -f pkcs7 --invisible --tsa "$TSA" --crl \
  --reason "PKCS7 + TSA explicit CRL"

run "32_pkcs7_tsa_ocsp" \
  -f pkcs7 --invisible --tsa "$TSA" --ocsp --no-crl \
  --reason "PKCS7 + TSA OCSP only"

run "33_pkcs7_tsa_crl_ocsp" \
  -f pkcs7 --invisible --tsa "$TSA" --crl --ocsp \
  --reason "PKCS7 + TSA CRL+OCSP"

run "34_pkcs7_tsa_dss" \
  -f pkcs7 --invisible --tsa "$TSA" --dss \
  --reason "PKCS7 + TSA with DSS"

run "35_pkcs7_tsa_crl_dss" \
  -f pkcs7 --invisible --tsa "$TSA" --crl --dss \
  --reason "PKCS7 + TSA CRL+DSS"

run "36_pkcs7_tsa_crl_ocsp_dss" \
  -f pkcs7 --invisible --tsa "$TSA" --crl --ocsp --dss \
  --reason "PKCS7 + TSA CRL+OCSP+DSS"

run "37_pkcs7_tsa_custom_field" \
  -f pkcs7 --invisible --tsa "$TSA" \
  --field "TSASig" \
  --reason "PKCS7 + TSA custom field"

run "38_pkcs7_tsa_full_metadata" \
  -f pkcs7 --invisible --tsa "$TSA" \
  --name "Alice TSA Dupont" \
  --contact "alice.tsa@corp.example" \
  --location "Paris, France" \
  --reason "PKCS7 + TSA full metadata"

run "39_pkcs7_tsa_visible_full_metadata" \
  -f pkcs7 --tsa "$TSA" -p 1 \
  --rect "30,680,280,740" \
  --name "Charlie TSA" \
  --contact "charlie.tsa@example.com" \
  --location "Tokyo" \
  --reason "PKCS7 + TSA visible all metadata"

# ── PAdES B-B + TSA (B-B normally has no TSA, but flag is still accepted) ────

run "40_pades_bb_tsa_invisible" \
  -f pades -l b-b --invisible \
  --tsa "$TSA" \
  --reason "PAdES B-B + TSA invisible"

run "41_pades_bb_tsa_visible" \
  -f pades -l b-b --tsa "$TSA" \
  --rect "50,700,250,750" \
  --name "Bob B-B TSA" \
  --reason "PAdES B-B + TSA visible"

# ── PAdES B-T + TSA ───────────────────────────────────────────────────────────

run "42_pades_bt_tsa_invisible" \
  -f pades -l b-t --invisible \
  --tsa "$TSA" \
  --reason "PAdES B-T + TSA invisible"

run "43_pades_bt_tsa_visible" \
  -f pades -l b-t --tsa "$TSA" \
  --rect "50,700,250,750" \
  --name "TSA B-T Signer" \
  --reason "PAdES B-T + TSA visible"

run "44_pades_bt_tsa_crl" \
  -f pades -l b-t --invisible --tsa "$TSA" --crl \
  --reason "PAdES B-T + TSA + CRL"

run "45_pades_bt_tsa_ocsp" \
  -f pades -l b-t --invisible --tsa "$TSA" --ocsp \
  --reason "PAdES B-T + TSA + OCSP"

run "46_pades_bt_tsa_full_metadata" \
  -f pades -l b-t --invisible --tsa "$TSA" \
  --name "Bob B-T TSA" \
  --contact "bob.bt@example.de" \
  --location "Berlin, Germany" \
  --reason "PAdES B-T + TSA full metadata"

# ── PAdES B-LT + TSA (DSS auto-enabled) ──────────────────────────────────────

run "47_pades_blt_tsa_invisible" \
  -f pades -l b-lt --invisible \
  --tsa "$TSA" \
  --reason "PAdES B-LT + TSA invisible"

run "48_pades_blt_tsa_visible" \
  -f pades -l b-lt --tsa "$TSA" \
  --rect "50,700,250,750" \
  --name "LT TSA Signer" \
  --reason "PAdES B-LT + TSA visible"

run "49_pades_blt_tsa_crl" \
  -f pades -l b-lt --invisible --tsa "$TSA" --crl \
  --reason "PAdES B-LT + TSA + CRL"

run "50_pades_blt_tsa_crl_ocsp" \
  -f pades -l b-lt --invisible --tsa "$TSA" --crl --ocsp \
  --reason "PAdES B-LT + TSA + CRL+OCSP"

run "51_pades_blt_tsa_full_metadata" \
  -f pades -l b-lt --invisible --tsa "$TSA" \
  --name "Carol LT TSA" \
  --contact "carol.lt@example.com" \
  --location "London, UK" \
  --reason "PAdES B-LT + TSA full metadata"

# ── PAdES B-LTA + TSA (highest level: DSS + doc timestamp) ───────────────────

run "52_pades_blta_tsa_invisible" \
  -f pades -l b-lta --invisible \
  --tsa "$TSA" \
  --reason "PAdES B-LTA + TSA invisible"

run "53_pades_blta_tsa_visible" \
  -f pades -l b-lta --tsa "$TSA" \
  --rect "50,700,250,750" \
  --name "LTA TSA Signer" \
  --reason "PAdES B-LTA + TSA visible"

run "54_pades_blta_tsa_full_metadata" \
  -f pades -l b-lta --invisible --tsa "$TSA" \
  --name "Dave LTA TSA" \
  --contact "dave.lta@example.com" \
  --location "New York, USA" \
  --crl --ocsp \
  --reason "PAdES B-LTA + TSA full metadata CRL+OCSP"

# ── Summary ──────────────────────────────────────────────────────────────────
printf '\n\n════════════════════════════════════════════════════════\n'
printf '  SUMMARY\n'
printf '════════════════════════════════════════════════════════\n'
printf '  ✅ Passed : %d\n' "$PASS"
printf '  ❌ Failed : %d\n' "$FAIL"
if [ ${#ERRORS[@]} -gt 0 ]; then
  printf '  Failed variants:\n'
  for e in "${ERRORS[@]}"; do printf '    - %s\n' "$e"; done
fi
printf '\n  Output files:\n'
ls -lh "$OUT"/*.pdf 2>/dev/null | awk '{printf "    %-10s  %s\n", $5, $NF}'
printf '════════════════════════════════════════════════════════\n'


import re, zlib, os

def analyze_pdf(path, label):
    data = open(path, 'rb').read()
    out = []
    out.append("=" * 70)
    out.append("FILE: %s (%s)" % (label, os.path.basename(path)))
    out.append("Size: %d bytes" % len(data))
    out.append("Header: %r" % data[:16])

    obj_defs = list(re.finditer(rb'\n(\d+)\s+(\d+)\s+obj\b', data))
    out.append("Object defs: %d" % len(obj_defs))

    types = {}
    for m in re.finditer(rb'/Type\s*/(\w+)', data):
        t = m.group(1).decode('latin1')
        types[t] = types.get(t, 0) + 1
    out.append("Types: %s" % types)

    filters = {}
    for m in re.finditer(rb'/Filter\s*/(\w+)', data):
        f = m.group(1).decode('latin1')
        filters[f] = filters.get(f, 0) + 1
    out.append("Filters: %s" % filters)

    images = []
    for m in re.finditer(rb'/Subtype\s*/Image', data):
        p = m.start()
        before = data[max(0,p-300):p]
        obj_m = list(re.finditer(rb'(\d+)\s+(\d+)\s+obj', before))
        if obj_m:
            lm = obj_m[-1]
            chunk = data[max(0,p-300):p+300]
            w = re.search(rb'/Width\s+(\d+)', chunk)
            h = re.search(rb'/Height\s+(\d+)', chunk)
            cs = re.search(rb'/ColorSpace\s*/(\w+)', chunk)
            bpc = re.search(rb'/BitsPerComponent\s+(\d+)', chunk)
            filt = re.search(rb'/Filter\s*/(\w+)', chunk)
            ln = re.search(rb'/Length\s+(\d+)', chunk)
            images.append({
                'obj': '%s %s' % (lm.group(1).decode(), lm.group(2).decode()),
                'w': int(w.group(1)) if w else '?',
                'h': int(h.group(1)) if h else '?',
                'cs': cs.group(1).decode() if cs else '?',
                'bpc': int(bpc.group(1)) if bpc else '?',
                'filter': filt.group(1).decode() if filt else '?',
                'length': int(ln.group(1)) if ln else '?',
            })
    out.append("Images (%d):" % len(images))
    for img in images:
        out.append("  obj %s: %sx%s cs=%s bpc=%s filter=%s len=%s" % (
            img['obj'], img['w'], img['h'], img['cs'], img['bpc'], img['filter'], img['length']))

    # Trailer
    for m in re.finditer(rb'trailer', data[-3000:]):
        pos = data.__len__() - 3000 + m.start()
        snip = data[pos:pos+200]
        out.append("Trailer: %r" % snip)
        break

    xref_kw = len(re.findall(rb'\nxref\b', data))
    out.append("Xref tables: %d" % xref_kw)
    stream_count = len(re.findall(rb'\bstream\b', data))
    out.append("Streams: %d" % stream_count)

    # Check first page content stream
    for m in re.finditer(rb'/Type\s*/Page\b', data):
        p = m.start()
        chunk = data[p:p+500]
        contents_m = re.search(rb'/Contents\s+(\d+)\s+(\d+)\s+R', chunk)
        if contents_m:
            out.append("First Page /Contents: %s %s R" % (
                contents_m.group(1).decode(), contents_m.group(2).decode()))
        break

    out.append("")
    return "\n".join(out)

result = ""
result += analyze_pdf('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'ORIGINAL')
result += analyze_pdf('/Users/fdimarh/Downloads/compressed rust-pdfbox/out_recommended.pdf', 'RUST-PDFBOX')
result += analyze_pdf('/Users/fdimarh/Downloads/compressed ilovepdf/2024sk-kma260_KMA_SK.KP5_XII_2024_recomended_compressed.pdf', 'ILOVEPDF')

with open('/Users/fdimarh/Documents/Lab/Rustlab/pdf/rust-pdfbox/tools/pdf_diff.txt', 'w') as f:
    f.write(result)
print("done")


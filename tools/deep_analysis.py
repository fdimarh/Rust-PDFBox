import re, zlib, os, struct

orig = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()
ilove = open('/Users/fdimarh/Downloads/compressed ilovepdf/2024sk-kma260_KMA_SK.KP5_XII_2024_recomended_compressed.pdf', 'rb').read()
rust = open('/Users/fdimarh/Downloads/compressed rust-pdfbox/out_recommended.pdf', 'rb').read()

def find_images_in_file(data, label):
    images = []
    for m in re.finditer(rb'/Subtype\s*/Image', data):
        p = m.start()
        before = data[max(0,p-300):p]
        obj_m = list(re.finditer(rb'(\d+)\s+(\d+)\s+obj', before))
        if obj_m:
            lm = obj_m[-1]
            chunk = data[max(0,p-300):p+600]
            w = re.search(rb'/Width\s+(\d+)', chunk)
            h = re.search(rb'/Height\s+(\d+)', chunk)
            cs = re.search(rb'/ColorSpace\s*/(\w+)', chunk)
            bpc = re.search(rb'/BitsPerComponent\s+(\d+)', chunk)
            filt = re.search(rb'/Filter\s*/(\w+)', chunk)
            ln = re.search(rb'/Length\s+(\d+)', chunk)
            images.append({
                'obj': int(lm.group(1)),
                'w': int(w.group(1)) if w else 0,
                'h': int(h.group(1)) if h else 0,
                'cs': cs.group(1).decode() if cs else '?',
                'bpc': int(bpc.group(1)) if bpc else 0,
                'filter': filt.group(1).decode() if filt else '?',
                'length': int(ln.group(1)) if ln else 0,
            })
    print("\n=== %s ===" % label)
    print("  Total images: %d" % len(images))
    total = sum(i['length'] for i in images)
    print("  Total image bytes: %d (%.1f MB)" % (total, total/1048576))
    by_filter = {}
    for img in images:
        f = img['filter']
        by_filter[f] = by_filter.get(f, {'count':0,'bytes':0,'dims':[]})
        by_filter[f]['count'] += 1
        by_filter[f]['bytes'] += img['length']
        by_filter[f]['dims'].append((img['w'], img['h']))
    for f, d in sorted(by_filter.items()):
        avg_w = sum(x[0] for x in d['dims']) // len(d['dims'])
        avg_h = sum(x[1] for x in d['dims']) // len(d['dims'])
        print("  %-20s: %d images, %d bytes (%.1f MB), avg %dx%d px" % (
            f, d['count'], d['bytes'], d['bytes']/1048576, avg_w, avg_h))

find_images_in_file(orig, 'ORIGINAL')
find_images_in_file(ilove, 'ILOVEPDF recommended')
find_images_in_file(rust, 'RUST-PDFBOX recommended')


#!/usr/bin/env python3
"""Check why JBIG2 images are not getting DPI from CTM scan."""
import re, os

pdf = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()

# Find JBIG2 object IDs
jbig2_ids = []
for m in re.finditer(rb'(\d+)\s+0\s+obj\s*<<[^>]*?/Filter\s*/JBIG2Decode', pdf, re.DOTALL):
    jbig2_ids.append(int(m.group(1)))
print(f"JBIG2 object IDs: {sorted(jbig2_ids)[:10]}...")

# Find all page content streams and look for cm + Do operators
pages = re.findall(rb'/Type\s*/Page\b', pdf)
print(f"Page count: {len(pages)}")

# Look for Do operators near resource references to understand what names are used
for m in re.finditer(rb'(\d+)\s+0\s+obj\s*<<[^>]*?/Type\s*/Page\b', pdf, re.DOTALL):
    obj_id = int(m.group(1))
    pos = m.start()
    page_chunk = pdf[pos:pos+2000]

    # Find content stream reference
    contents = re.search(rb'/Contents\s+(\d+)\s+0\s+R', page_chunk)
    if not contents:
        continue

    content_id = int(contents.group(1))

    # Find the content stream
    for cp in re.finditer(('\n%d 0 obj' % content_id).encode(), pdf):
        cpos = cp.start()
        chunk = pdf[cpos:cpos+500]
        ln = re.search(rb'/Length\s+(\d+)', chunk)
        if not ln:
            continue
        length = int(ln.group(1))
        stream_start = chunk.find(b'stream')
        if stream_start < 0:
            continue
        abs_start = cpos + stream_start + 6
        if pdf[abs_start] == ord('\r'): abs_start += 1
        if pdf[abs_start] == ord('\n'): abs_start += 1
        content = pdf[abs_start:abs_start+length]

        # Look for cm and Do operators
        try:
            text = content.decode('latin1')
        except:
            continue

        tokens = text.split()
        cm_found = False
        for i, tok in enumerate(tokens):
            if tok == 'cm' and i >= 6:
                cm_found = True
                matrix = tokens[i-6:i]
                print(f"  Page obj {obj_id}, content {content_id}: cm = {matrix}")
            if tok == 'Do' and i >= 1:
                xobj_name = tokens[i-1]
                print(f"  Page obj {obj_id}: Do {xobj_name} (cm_before={cm_found})")
        break
    break  # Only check first page

print("\n--- Resources of first page ---")
for m in re.finditer(rb'(\d+)\s+0\s+obj\s*<<[^>]*?/Type\s*/Page\b', pdf, re.DOTALL):
    pos = m.start()
    page_chunk = pdf[pos:pos+3000]
    xobjects = re.findall(rb'/XObject\s*<<([^>]+)>>', page_chunk)
    if xobjects:
        print("XObject dict:", xobjects[0][:200])
    break


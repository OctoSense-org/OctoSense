#!/usr/bin/env python3
"""PDF Tools' dev fixture: sample documents for UI tests in App Hub's card-host.

It never ships. `apps/pdftools/tests/ui.py` runs it to fill a card-host app
storage folder (PDF Tools' jail) with:

- accounts/device/library/*.pdf: small stand-in PDFs, so Home lists them;
- dev/pages/<id>/<n>.png: each page drawn here with PIL at 2 pixels per PDF
  point, the size the app asks for at 100%;
- dev/fixture.json: what the stand-in engine (engine.splash) answers for
  each document: pages and sizes, outline, form fields and comments;
- dev/lines/<id>/<n>.json: each page's text lines and paragraphs with their
  boxes in points.

The page text is the approved designs' sample text (design/source/), so a
grab can be put beside its design. Usage: make_fixture.py <jail folder>
"""
import json
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

S = 2  # pixels per point
LETTER = (612, 792)
FONTS = {
    "serif": ["/System/Library/Fonts/Supplemental/Georgia.ttf", "/System/Library/Fonts/Supplemental/Times New Roman.ttf"],
    "serif-bold": ["/System/Library/Fonts/Supplemental/Georgia Bold.ttf"],
    "sans": ["/System/Library/Fonts/Supplemental/Arial.ttf"],
    "sans-bold": ["/System/Library/Fonts/Supplemental/Arial Bold.ttf"],
}
NAMES = {"serif": "Georgia", "serif-bold": "Georgia-Bold", "sans": "Arial", "sans-bold": "Arial-Bold"}
BLUE = (27, 71, 144)
INK = (40, 40, 42)
GREY = (120, 122, 126)
_cache = {}


def font(kind, size):
    key = (kind, size)
    if key not in _cache:
        found = None
        for path in FONTS[kind]:
            if Path(path).exists():
                found = ImageFont.truetype(path, int(round(size * S)))
                break
        _cache[key] = found or ImageFont.load_default()
    return _cache[key]


class Page:
    def __init__(self, size=LETTER):
        self.size = size
        self.img = Image.new("RGB", (size[0] * S, size[1] * S), "white")
        self.draw = ImageDraw.Draw(self.img)
        self.lines = []
        self.paragraphs = []

    def text(self, x, y, text, kind="serif", size=11, color=INK, record=True):
        f = font(kind, size)
        self.draw.text((x * S, y * S), text, font=f, fill=color)
        w = f.getlength(text) / S
        if record:
            self.lines.append({"n": len(self.lines) + 1, "text": text, "box": [round(x, 1), round(y, 1), round(w, 1), round(size * 1.25, 1)],
                               "font": NAMES[kind], "size": size})
        return w

    def para(self, x, y, width, text, kind="serif", size=11, leading=1.55, color=INK):
        f = font(kind, size)
        words, line, rows = text.split(), "", []
        for word in words:
            trial = (line + " " + word).strip()
            if f.getlength(trial) / S > width and line:
                rows.append(line)
                line = word
            else:
                line = trial
        if line:
            rows.append(line)
        first = len(self.lines) + 1
        top = y
        for row in rows:
            self.text(x, y, row, kind, size, color)
            y += size * leading
        self.paragraphs.append({"n": len(self.paragraphs) + 1, "text": " ".join(rows), "box": [x, round(top, 1), width, round(y - top, 1)],
                                "lines": list(range(first, len(self.lines) + 1)), "font": NAMES[kind], "size": size})
        return y

    def rule(self, x, y, w, color=(170, 172, 178)):
        self.draw.line([(x * S, y * S), (x * S + w * S, y * S)], fill=color, width=S)

    def box(self, x, y, w, h, fill=None, outline=None):
        self.draw.rectangle([x * S, y * S, (x + w) * S, (y + h) * S], fill=fill, outline=outline, width=S)

    def mark(self, line, color=(250, 226, 89)):
        """Paint a highlight under a recorded line, as a saved annotation looks."""
        x, y, w, h = line["box"]
        layer = Image.new("RGBA", self.img.size, (0, 0, 0, 0))
        ImageDraw.Draw(layer).rectangle([x * S - 2, y * S, (x + w) * S + 2, (y + h) * S], fill=color + (150,))
        self.img = Image.alpha_composite(self.img.convert("RGBA"), layer).convert("RGB")
        self.draw = ImageDraw.Draw(self.img)
        self.draw.text((x * S, y * S), line["text"], font=font("serif", line["size"]), fill=INK)

    def answer(self):
        return {"lines": self.lines, "paragraphs": self.paragraphs}


def report_header(p, n):
    p.text(54, 44, "Northwind Cooperative", "sans", 8.5, BLUE)
    p.text(450, 44, "Q3 2026 Board Report", "sans", 8.5, GREY)
    p.text(552, 752, str(n), "sans", 9, GREY)


def chart(p, x, y, values, labels, top):
    w, h = 300, 150
    p.rule(x + 20, y + h, w)
    for k in range(5):
        gy = y + h - k * h / 4
        p.text(x, gy - 6, str(int(top * k / 4)), "sans", 7, GREY, record=False)
    step = w / len(values)
    for i, v in enumerate(values):
        bh = h * v / top
        bx = x + 30 + i * step
        p.box(bx, y + h - bh, step * 0.5, bh, fill=(22, 106, 217))
        p.text(bx + 2, y + h - bh - 13, str(v), "sans", 8, INK, record=False)
        p.text(bx - 4, y + h + 6, labels[i], "sans", 8, INK, record=False)


SECTIONS = [
    ("Summary", "Northwind Cooperative delivered a solid performance in Q3 2026, with strong revenue growth, disciplined cost management and continued member value."),
    ("Revenue", "Revenue for the third quarter of 2026 was $28.7 million, up from $25.6 million in Q3 2025. Revenue grew 12% year over year, led by the Northwest region."),
    ("Operations", "Operational performance remained strong in Q3 2026. We continued to improve operational efficiency and service reliability."),
    ("Outlook", "We are optimistic about the remainder of 2026 and beyond, with continued revenue growth and investments in grid modernization."),
    ("Appendix", "A. Financial Statements. B. Key Metrics. C. Glossary. D. Notes."),
]


def board_report():
    pages = []
    # 1: cover
    p = Page()
    p.text(54, 90, "NORTHWIND", "sans-bold", 20, BLUE)
    p.text(54, 116, "COOPERATIVE", "sans", 9, BLUE)
    p.text(54, 200, "Q3 2026", "sans-bold", 30, INK)
    p.text(54, 238, "Board Report", "sans-bold", 30, INK)
    p.text(54, 290, "Building strong communities through cooperative growth.", "sans", 11, GREY)
    p.box(0, 520, 612, 272, fill=(84, 132, 196))
    p.text(54, 470, "Prepared for the Board of Directors, October 20, 2026", "sans", 9, GREY)
    pages.append(p)
    # 2: contents / summary
    p = Page()
    report_header(p, 2)
    p.text(54, 80, "Summary", "sans-bold", 26, INK)
    p.rule(54, 120, 504)
    y = p.para(54, 140, 470, SECTIONS[0][1], size=12)
    y = p.para(54, y + 14, 470, "The board met on 21 September 2026 to review the quarter's results.", size=12)
    y = p.para(54, y + 14, 470, "We advanced key operational initiatives across our regions and reinforced our focus on member service, product quality and sustainable practices.", size=12)
    p.para(54, y + 14, 470, "This report summarises financial results, operations highlights and our outlook for the remainder of the year.", size=12)
    pages.append(p)
    # 3: revenue with the chart and a saved highlight
    p = Page()
    report_header(p, 3)
    p.text(54, 78, "Revenue", "sans-bold", 26, BLUE)
    p.rule(54, 118, 504)
    y = p.para(54, 136, 480, SECTIONS[1][1], size=12)
    highlighted = p.lines[p.paragraphs[0]["lines"][1] - 1]
    p.mark(highlighted)
    y = p.para(54, y + 10, 480, "Strong customer demand and disciplined pricing supported broad-based growth across all regions.", size=12)
    p.text(54, y + 18, "Revenue by region, Q3 2026", "sans-bold", 12, BLUE)
    p.text(54, y + 36, "Millions of USD", "sans", 8, GREY, record=False)
    chart(p, 54, y + 56, [12.4, 7.1, 5.6, 3.6], ["Northwest", "Coast", "Valley", "Metro"], 14)
    p.para(54, y + 250, 480, "The Northwest region contributed 43% of total revenue, supported by strong performance in commercial and agricultural segments.", size=12)
    pages.append(p)
    # 4..24: the other sections, then numbered notes
    for n in range(4, 25):
        p = Page()
        report_header(p, n)
        title, body = SECTIONS[(n - 2) % len(SECTIONS)] if n <= 6 else ("Notes, part " + str(n - 6), "This page continues the notes to the report. Revenue and costs are shown for the quarter, with comparisons to the same period last year." if n in (9, 15, 16, 21) else "This page continues the notes to the report, with the figures the board asked for at its last meeting.")
        p.text(54, 78, title, "sans-bold", 24, BLUE)
        p.rule(54, 116, 504)
        y = p.para(54, 136, 480, body, size=12)
        for k in range(4):
            y = p.para(54, y + 12, 480, "Members, staff and partners worked together through the quarter. The cooperative kept its commitments while preparing for the year ahead.", size=12)
        pages.append(p)
    outline = [{"title": "Summary", "page": 2, "children": []},
               {"title": "Revenue", "page": 3, "children": [{"title": "Revenue by region", "page": 3, "children": []}]},
               {"title": "Operations", "page": 4, "children": []},
               {"title": "Outlook", "page": 5, "children": []},
               {"title": "Appendix", "page": 6, "children": []}]
    third = pages[2]
    rev = third.lines[third.paragraphs[0]["lines"][1] - 1]["box"]
    comments = [{"id": "c1", "page": 3, "type": "highlight", "author": "Jun Park", "text": "Can we add the regional breakdown table here?", "date": "2026-10-10T10:12",
                 "color": "#FFD84D", "status": "accepted", "rects": [rev],
                 "replies": [{"id": "r1", "author": "Maya Chen", "text": "Added on page 4.", "date": "2026-10-10T10:20"}]},
                {"id": "c2", "page": 7, "type": "note", "author": "Ana Ruiz", "text": "Please check these figures against the audited accounts.", "date": "2026-10-09T15:30",
                 "color": "#2D7FF9", "status": "none", "rects": [[480, 140, 18, 18]], "replies": []}]
    return pages, outline, comments, []


def lease():
    pages = []
    p = Page()
    p.text(200, 60, "RIVERSIDE RESIDENCES", "sans-bold", 18, INK)
    p.text(230, 86, "RESIDENTIAL LEASE AGREEMENT", "sans", 9, INK)
    y = p.para(54, 120, 500, "This Residential Lease Agreement (\"Agreement\") is made between Riverside Residences LLC (\"Landlord\") and the Tenant named below.", size=10.5)
    rows = [("1. PARTIES", [("Tenant name:", "Tenant name"), ("Landlord:", None)]), ("2. PREMISES", [("Address:", None), ("Unit:", "Unit")]),
            ("3. TERM", [("Start date:", "Start date"), ("End date:", "End date")]), ("4. RENT", [("Monthly rent: $", "Monthly rent")])]
    fields = []
    y += 14
    for head, items in rows:
        p.text(54, y, head, "sans-bold", 10)
        y += 24
        for label, field in items:
            p.text(70, y, label, "serif", 10)
            if field:
                rect = [160, y - 3, 210 if field in ("Tenant name", "Monthly rent") else 110, 18]
                fields.append({"name": field, "type": "text", "value": {"Tenant name": "Maya Chen", "Unit": "4B", "Start date": "1 November 2026"}.get(field, ""),
                               "required": field == "Monthly rent", "read_only": False, "page": 1, "rect": rect, "options": []})
            else:
                p.text(160, y, {"Landlord:": "Riverside Residences LLC", "Address:": "123 Riverfront Drive, Portland, OR 97201"}[label], "serif", 10)
            y += 26
        y += 6
    p.text(54, y, "5. SIGNATURES", "sans-bold", 10)
    y = p.para(70, y + 22, 470, "By signing below, the parties agree to the terms of this Agreement.", size=10)
    p.text(70, y + 10, "Landlord", "serif-bold", 10)
    p.text(330, y + 10, "Tenant", "serif-bold", 10)
    p.text(70, y + 70, "Initials:", "serif", 10)
    p.text(330, y + 70, "Initials:", "serif", 10)
    fields.append({"name": "Initials (Landlord)", "type": "text", "value": "", "required": True, "read_only": False, "page": 1, "rect": [120, y + 66, 32, 20], "options": []})
    fields.append({"name": "Initials (Tenant)", "type": "text", "value": "", "required": True, "read_only": False, "page": 1, "rect": [380, y + 66, 32, 20], "options": []})
    p.text(54, 752, "Page 1 of 2", "serif", 9, GREY)
    pages.append(p)
    p = Page()
    p.para(54, 80, 500, "6. RULES. The Tenant agrees to follow the building rules attached to this Agreement.", size=10.5)
    p.text(54, 752, "Page 2 of 2", "serif", 9, GREY)
    pages.append(p)
    return pages, [], [], fields


def simple(title, count, lead):
    pages = []
    for n in range(1, count + 1):
        p = Page()
        p.text(54, 70, title if n == 1 else title + ", page " + str(n), "serif-bold", 22 if n == 1 else 14, INK)
        y = p.para(54, 120, 480, lead, size=11)
        p.para(54, y + 12, 480, "This sample page stands in for the engine's render in PDF Tools' card-host tests.", size=11)
        pages.append(p)
    return pages, [], [], []


DOCS = [
    ("Q3 2026 Board Report", "board", board_report, 2936013),
    ("Riverside Lease 2026", "lease", lease, 1153434),
    ("Field Guide to Garden Birds", "birds", lambda: simple("Field Guide to Garden Birds", 7, "Identify common birds in your backyard and local parks."), 16384),
    ("Invoice INV-2041", "invoice", lambda: simple("INVOICE", 1, "INV-2041. Bill to: Northwind Cooperative, 123 Harbor Way, Portland, OR 97201. Consulting Services $2,400.00"), 86016),
    ("Site Survey Photos", "survey", lambda: simple("Site Survey Photos", 4, "Project: Community Center. Date: September 25, 2026."), 10066330),
    ("Appendix A - Figures", "appendix", lambda: simple("Appendix A - Figures", 9, "Figures for the Q3 2026 Board Report."), 412000),
    ("Cover Letter", "letter", lambda: simple("Cover Letter", 2, "Dear members of the board, please find the quarter's report enclosed."), 52000),
]


def main():
    jail = Path(sys.argv[1])
    library = jail / "accounts/device/library"
    library.mkdir(parents=True, exist_ok=True)
    out = {"docs": {}, "damaged": ["Damaged scan.pdf"], "protected": ["Payroll 2026.pdf"], "you": "Maya Chen"}
    for title, ident, make, size in DOCS:
        pages, outline, comments, fields = make()
        folder = jail / "dev/pages" / ident
        folder.mkdir(parents=True, exist_ok=True)
        for n, p in enumerate(pages, 1):
            p.img.save(folder / f"{n}.png", optimize=True)
        small = pages[0].img.resize((pages[0].img.width // 8, pages[0].img.height // 8))
        small.save(library / f"{title}.pdf")
        # Each page's lines in a small file of its own: the stand-in reads one
        # when asked, inside one handler's 64 ms.
        lines = jail / "dev/lines" / ident
        lines.mkdir(parents=True, exist_ok=True)
        for n, p in enumerate(pages, 1):
            (lines / f"{n}.json").write_text(json.dumps(p.answer()))
        out["docs"][f"{title}.pdf"] = {
            "id": ident, "title": title, "file_size": size,
            "sizes": [list(p.size) for p in pages], "outline": outline, "comments": comments, "fields": fields,
        }
    for name in out["damaged"] + out["protected"]:
        Image.new("RGB", (60, 80), "white").save(library / name)
    (jail / "dev/fixture.json").write_text(json.dumps(out))
    # The library index as the design shows Home: when each was last opened.
    now = time.time()
    def at(month, day):
        return time.mktime((2026, month, day, 10, 0, 0, 0, 0, -1))
    opened = {"Q3 2026 Board Report": now - 3600, "Riverside Lease 2026": now - 86400,
              "Field Guide to Garden Birds": at(10, 3), "Invoice INV-2041": at(10, 1), "Site Survey Photos": at(9, 28)}
    docs = {f"{title}.pdf": {"opened": stamp} for title, stamp in opened.items()}
    (library.parent / "library.json").write_text(json.dumps({"version": 1, "docs": docs}))
    print(f"{len(out['docs'])} documents in {jail}")


if __name__ == "__main__":
    main()

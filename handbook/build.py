#!/usr/bin/env python3
"""Build Lumen-Handbook.pdf and the docs website (../docs) from handbook.src.html.

Every <lumen> block is run through the installed `lumen` binary and its real
output is placed under the code, so the handbook cannot drift from the language.
The build fails if an example does not exit the way it is marked.

    <lumen>        run, must succeed, show output
    <lumen fail>   run, must exit 1, show the error
    <lumen norun>  highlight only
    <shell>        terminal transcript (static)
    <plain>        any other code (static)

Needs: lumen, google-chrome, pdftotext.
"""
import html
import pathlib
import re
import shutil
import subprocess
import sys
import textwrap

HERE = pathlib.Path(__file__).resolve().parent
BUILD = HERE / ".build"
OUT = HERE / "Lumen-Handbook.pdf"
SITE = HERE.parent / "docs"  # served by GitHub Pages
WEBSITE = "https://lumen-lang.vercel.app"
REPO = "https://github.com/ertodebana/lumen"
LUMEN = shutil.which("lumen") or sys.exit("build.py: `lumen` is not on PATH (cargo install --path .)")
CHROME = shutil.which("google-chrome") or shutil.which("chromium") or sys.exit("build.py: Chrome not found")

TOKEN = re.compile(
    r'(?P<comment>#[^\n]*)'
    r'|(?P<string>"(?:\\.|[^"\\])*")'
    r'|(?P<number>\b\d+(?:\.\d+)?\b)'
    r'|(?P<kw>\b(?:let|fn|return|if|else|while|print|and|or|not)\b)'
    r'|(?P<const>\b(?:true|false|nil)\b)'
    r'|(?P<call>\b[A-Za-z_]\w*(?=\())'
)


def esc(text):
    return html.escape(text, quote=False)


def clean(body):
    return textwrap.dedent(body).strip("\n")


def highlight(code):
    out, pos = [], 0
    for m in TOKEN.finditer(code):
        out.append(esc(code[pos:m.start()]))
        out.append(f'<span class="{m.lastgroup}">{esc(m.group())}</span>')
        pos = m.end()
    out.append(esc(code[pos:]))
    return "".join(out)


def lumen_block(m):
    flags, code = m.group(1).split(), clean(m.group(2))
    parts = [f'<pre class="src">{highlight(code)}</pre>']
    if "norun" not in flags:
        r = subprocess.run([LUMEN, "-c", code], capture_output=True, text=True, timeout=30)
        want = 1 if "fail" in flags else 0
        if r.returncode != want:
            sys.exit(f"build.py: example exited {r.returncode}, expected {want}:\n{code}\n{r.stderr}")
        if r.stdout:
            parts.append(f'<pre class="out">{esc(r.stdout.rstrip())}</pre>')
        if r.stderr:
            parts.append(f'<pre class="out err">{esc(r.stderr.rstrip())}</pre>')
    return f'<figure class="code">{"".join(parts)}</figure>'


def shell_block(m):
    lines = []
    for line in clean(m.group(1)).split("\n"):
        for prompt in ("$ ", "> ", "... "):
            if line.startswith(prompt) or line == prompt.strip():
                rest = line[len(prompt):]
                lines.append(f'<span class="prompt">{esc(prompt)}</span><span class="cmd">{esc(rest)}</span>')
                break
        else:
            lines.append(esc(line))
    return f'<figure class="code"><pre class="src shell">{chr(10).join(lines)}</pre></figure>'


def plain_block(m):
    return f'<figure class="code"><pre class="src">{esc(clean(m.group(1)))}</pre></figure>'


chapters = []  # (id, label, number, title, lead)


def chapter_block(m):
    cid, n, title, lead = m.group(1), m.group(2), m.group(3), m.group(4).strip()
    label = f"CHAPTER {n}" if n.isdigit() else f"APPENDIX {n}"
    chapters.append((cid, label, n, title, lead))
    return (
        f'<header class="chapter" id="{cid}"><div class="eyebrow">{label}</div>'
        f'<h1>{title}</h1><div class="rule"></div><p class="lead">{lead}</p></header>'
    )


def toc():
    rows = []
    for cid, label, n, title, _ in chapters:
        kind = "" if n.isdigit() else " appendix"
        rows.append(
            f'<a class="toc-row{kind}" href="#{cid}"><span class="toc-n">{n}</span>'
            f'<span class="toc-t">{title}</span><span class="toc-dots"></span>'
            f'<span class="toc-p">{{{{p:{cid}}}}}</span></a>'
        )
    return "\n".join(rows)


PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} | Lumen docs</title>
<meta name="description" content="{description}">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%230e1a2b'/%3E%3Ccircle cx='16' cy='16' r='7' fill='%23f0a020'/%3E%3C/svg%3E">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,300..800&family=JetBrains+Mono:ital,wght@0,400;0,500;1,400&display=swap" rel="stylesheet">
<link rel="stylesheet" href="docs.css">
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<div class="sidebar">
  <a class="brand" href="index.html">lumen <span>docs</span></a>
  <nav aria-label="Chapters">
{nav}
  </nav>
  <div class="elsewhere">
    <a href="{website}">Website</a>
    <a href="{repo}">GitHub</a>
    <a href="Lumen-Handbook.pdf">Handbook (PDF)</a>
  </div>
</div>
<main id="main">
{body}
{pager}
<footer>Lumen {version}. Every example on this site was run by the <code>lumen</code> executable when the page was built.</footer>
</main>
</body>
</html>
"""


def plain(text):
    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", text)).strip()


def build_site(doc, version):
    """One HTML page per chapter, cut from the same processed document as the PDF."""
    SITE.mkdir(exist_ok=True)
    body = doc.split("<body>", 1)[1].rsplit("</body>", 1)[0]
    body = re.sub(r"<!-- =+ .*? =+ -->", "", body)
    body = body.split('<div class="colophon">')[0]
    parts = re.split(r'(?=<header class="chapter")', body)[1:]
    assert len(parts) == len(chapters), (len(parts), len(chapters))

    def label_of(n):
        return f"Chapter {int(n)}" if n.isdigit() else f"Appendix {n}"

    def nav(current):
        rows = ['    <a href="index.html"' + (' aria-current="page"' if current is None else "") + ">Overview</a>"]
        for cid, _, n, title, _ in chapters:
            here = ' aria-current="page"' if cid == current else ""
            num = str(int(n)) if n.isdigit() else n
            rows.append(f'    <a href="{cid}.html"{here}><span>{num}</span>{title}</a>')
        return "\n".join(rows)

    def write(name, title, description, current, body_html, pager):
        (SITE / name).write_text(PAGE.format(
            title=title, description=html.escape(description), nav=nav(current), body=body_html,
            pager=pager, version=version, website=WEBSITE, repo=REPO), encoding="utf-8")

    for i, (part, (cid, label, n, title, lead)) in enumerate(zip(parts, chapters)):
        part = part.replace(f'<div class="eyebrow">{label}</div>', f'<p class="kicker">{label_of(n)}</p>')
        part = re.sub(r'id="arr(\d?)"', r'id="arr\1"', part)
        links = []
        if i > 0:
            links.append(f'<a class="prev" href="{chapters[i - 1][0]}.html"><small>Previous</small>{chapters[i - 1][3]}</a>')
        else:
            links.append('<a class="prev" href="index.html"><small>Previous</small>Overview</a>')
        if i + 1 < len(chapters):
            links.append(f'<a class="next" href="{chapters[i + 1][0]}.html"><small>Next</small>{chapters[i + 1][3]}</a>')
        write(f"{cid}.html", title, plain(lead), cid, part, f'<nav class="pager" aria-label="Pages">{"".join(links)}</nav>')

    hello = lumen_block(re.match(r"<lumen([^>]*)>\n(.*?)</lumen>", '<lumen>\nfn greet(name) {\n    return "Hello, " + name\n}\n\nprint greet("world")\n</lumen>', re.S))
    cards = "\n".join(
        f'<a class="entry" href="{cid}.html"><span>{label_of(n)}</span><strong>{title}</strong><em>{plain(lead)}</em></a>'
        for cid, _, n, title, lead in chapters)
    overview = f"""<header class="chapter"><p class="kicker">Lumen {version}</p><h1>Documentation</h1><div class="rule"></div>
<p class="lead">Lumen is a small, dynamically typed language with closures and first-class functions.
It is not fast. These pages teach it from the first program to the full grammar.</p></header>
{hello}
<h2>Install</h2>
{shell_block(re.match(r"<shell>\n(.*?)</shell>", "<shell>\n$ git clone " + REPO + "\n$ cd lumen\n$ cargo install --path .\n$ lumen --version\nLumen " + version + "\n</shell>", re.S))}
<p>You need a Rust toolchain. Chapter 1 covers the command line and the interactive prompt.</p>
<h2>Contents</h2>
<div class="entries">
{cards}
</div>
<p>Prefer paper? The same material is a 45-page <a href="Lumen-Handbook.pdf">handbook in PDF</a>.</p>"""
    first = chapters[0]
    write("index.html", "Documentation", "Documentation for the Lumen programming language.", None, overview,
          f'<nav class="pager" aria-label="Pages"><a class="next" href="{first[0]}.html"><small>Next</small>{first[3]}</a></nav>')

    shutil.copy(HERE / "site.css", SITE / "docs.css")
    (SITE / ".nojekyll").write_text("")
    return len(parts) + 1


def render(page_html, pdf):
    src = BUILD / "handbook.html"
    src.write_text(page_html, encoding="utf-8")
    subprocess.run(
        [CHROME, "--headless=new", "--disable-gpu", "--no-pdf-header-footer",
         f"--print-to-pdf={pdf}", src.as_uri()],
        check=True, capture_output=True, timeout=180,
    )


def main():
    BUILD.mkdir(exist_ok=True)
    doc = (HERE / "handbook.src.html").read_text(encoding="utf-8")
    version = subprocess.run([LUMEN, "--version"], capture_output=True, text=True).stdout.split()[-1]
    doc = doc.replace("{{version}}", version)
    doc = re.sub(r"<lumen([^>]*)>\n(.*?)</lumen>", lumen_block, doc, flags=re.S)
    doc = re.sub(r"<shell>\n(.*?)</shell>", shell_block, doc, flags=re.S)
    doc = re.sub(r"<plain>\n(.*?)</plain>", plain_block, doc, flags=re.S)
    doc = re.sub(r'<chapter id="(\w+)" n="(\w+)" title="([^"]+)">(.*?)</chapter>', chapter_block, doc, flags=re.S)
    site_pages = build_site(doc, version)
    doc = doc.replace("<!--TOC-->", toc())

    # Pass 1: find the page each chapter landed on. Pass 2: write those into the contents.
    draft = BUILD / "draft.pdf"
    render(re.sub(r"\{\{p:\w+\}\}", "00", doc), draft)
    pages = subprocess.run(["pdftotext", "-layout", str(draft), "-"], capture_output=True, text=True).stdout.split("\f")
    squashed = [re.sub(r"\s+", "", p) for p in pages]
    for cid, label, _, _, _ in chapters:
        key = label.replace(" ", "")
        hits = [i + 1 for i, p in enumerate(squashed) if key in p]
        if len(hits) != 1:
            sys.exit(f"build.py: could not locate '{label}' (found on pages {hits})")
        doc = doc.replace(f"{{{{p:{cid}}}}}", str(hits[0]))
    render(doc, OUT)
    shutil.copy(OUT, SITE / "Lumen-Handbook.pdf")
    print(f"{OUT.relative_to(HERE.parent)}: {len(pages) - 1} pages, {len(chapters)} chapters, lumen {version}")
    print(f"docs/: {site_pages} web pages")


if __name__ == "__main__":
    main()

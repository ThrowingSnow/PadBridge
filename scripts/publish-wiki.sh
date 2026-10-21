#!/bin/bash
# Publishes DOC.md to the GitHub wiki: one page per "## N. Title" section,
# the text before the first section as Home, plus a sidebar and a footer.
# Links between sections become wiki links. Edit DOC.md, not the wiki.
#
#   scripts/publish-wiki.sh            # build and push
#   scripts/publish-wiki.sh --dry-run  # build into a temp folder only
#
# The wiki repository exists only after its first page was created once on
# github.com (Wiki tab → "Create the first page").
set -euo pipefail
cd "$(dirname "$0")/.."

REPO=ThrowingSnow/PadBridge
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [ "${1:-}" = "--dry-run" ]; then
    mkdir -p "$WORK/wiki"
else
    git clone -q "https://github.com/$REPO.wiki.git" "$WORK/wiki"
fi

python3 - "$WORK/wiki" "$REPO" <<'PY'
import re, sys, pathlib

out = pathlib.Path(sys.argv[1]); repo = sys.argv[2]
doc = pathlib.Path("DOC.md").read_text()
blob = f"https://github.com/{repo}/blob/main/"

def anchor(title):
    # GitHub's heading anchors: lower case, drop punctuation, spaces to dashes.
    a = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
    return a

parts = re.split(r"^## (\d+)\. (.+)$", doc, flags=re.M)
intro, sections = parts[0], []
for i in range(1, len(parts), 3):
    num, title, body = parts[i], parts[i + 1].strip(), parts[i + 2]
    page = title.replace(" ", "-").replace("/", "-")
    sections.append((num, title, page, body))

# Where every anchor of DOC.md lives in the wiki.
target = {}
for num, title, page, body in sections:
    target[anchor(f"{num}. {title}")] = page
    for sub in re.findall(r"^### (.+)$", body, flags=re.M):
        target[anchor(sub)] = f"{page}#{anchor(sub)}"

def fix_links(text):
    def repl(m):
        label, url = m.group(1), m.group(2)
        if url.startswith("#"):
            return f"[{label}]({target.get(url[1:], 'Home')})"
        if not re.match(r"[a-z]+://", url):
            return f"[{label}]({blob}{url})"
        return m.group(0)
    text = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", repl, text)
    # Drop the --- separators at section ends.
    return re.sub(r"\n---\s*$", "\n", text.rstrip()) + "\n"

home = intro.replace("# Setting up PadBridge", "# PadBridge").rstrip()
home = re.sub(r"Contents:\n(?:\d+\. .*\n?)+", "", home + "\n")
# Page links are added after fix_links, which would treat them as repo files.
home = fix_links(home) + "\n## Pages\n\n" + "".join(f"{n}. [{t}]({p})\n" for n, t, p, _ in sections)
(out / "Home.md").write_text(home)

for num, title, page, body in sections:
    (out / f"{page}.md").write_text(fix_links(f"# {title}\n{body}"))

(out / "_Sidebar.md").write_text(
    "**[Home](Home)**\n\n" + "".join(f"{n}. [{t}]({p})\n" for n, t, p, _ in sections)
    + f"\n[Releases](https://github.com/{repo}/releases)\n")
(out / "_Footer.md").write_text(
    f"Generated from [DOC.md]({blob}DOC.md); edit it there. "
    f"Deutsch: [DOC.de.md]({blob}DOC.de.md)\n")
print("\n".join(sorted(p.name for p in out.glob("*.md"))))
PY

if [ "${1:-}" = "--dry-run" ]; then
    cp -R "$WORK/wiki" "${TMPDIR:-/tmp}/padbridge-wiki-preview"
    echo "preview: ${TMPDIR:-/tmp}/padbridge-wiki-preview"
    exit 0
fi

cd "$WORK/wiki"
git add -A
if git diff --cached --quiet; then
    echo "wiki unchanged"
else
    git commit -q -m "Update from DOC.md"
    git push -q
    echo "wiki updated: https://github.com/$REPO/wiki"
fi

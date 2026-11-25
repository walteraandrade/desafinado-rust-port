"""Writes the synthetic Markdown documents the performance scenarios open, in $PERF_DIR/docs."""
import os, random, sys
root = os.path.realpath(os.environ.get("PERF_DIR", os.path.expanduser("~/.cache/desafinado-perf")))
out = os.path.realpath(os.path.join(root, "docs"))
if not out.startswith(root + os.sep):
    sys.exit(f"{out} is not under {root}")
os.makedirs(out, exist_ok=True)
random.seed(7)
en = "the writer opens a draft and reads each paragraph slowly while the editor keeps every change as a tracked suggestion that waits for review before it becomes part of the text".split()
pt = "a escritora abre o rascunho e lê cada parágrafo com calma enquanto o editor guarda cada mudança como uma sugestão marcada que espera revisão antes de entrar no texto".split()
typos = ["teh","recieve","definately","enviroment","seperate","ocurrence","concider","paragrah"]
def para():
    words = en if random.random()<0.6 else pt
    n = random.randint(40,90)
    out = [random.choice(words) for _ in range(n)]
    for _ in range(random.randint(0,2)): out[random.randrange(n)] = random.choice(typos)
    s = " ".join(out); return s[0].upper()+s[1:]+"."
def doc(target):
    parts=["---\ntitle: Benchmark\ntags: [perf]\n---\n","# Benchmark document\n"]
    size=0; i=0
    while size<target:
        i+=1
        r=random.random()
        if i%12==0: p=f"## Section {i}\n"
        elif r<0.08: p="\n".join(f"- {para()[:80]}" for _ in range(4))
        elif r<0.12: p="```rust\nfn main() {\n    println!(\"hello {}\", 42);\n}\n```"
        elif r<0.15: p="> "+para()
        else: p=para()
        parts.append(p); size+=len(p)+2
    return "\n\n".join(parts)+"\n"
for name,target in [("small",2_000),("medium",50_000),("large",500_000),("huge",2_000_000)]:
    open(f"{out}/{name}.md","w").write(doc(target))
# One logical line of 60 KB: the worst case for soft wrap.
open(f"{out}/paragraph.md","w").write("# One paragraph\n\n" + " ".join(para() for _ in range(150)) + "\n")

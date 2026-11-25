"""Summarizes performance runs: python3 analyze.py RUN_DIR [RUN_DIR ...]"""
import json, sys, collections, os
def pct(xs, p):
    if not xs: return 0
    xs = sorted(xs); return xs[min(len(xs)-1, int(round(p/100*(len(xs)-1))))]
def ms(us): return round(us/1000, 2)
def threads(d):
    def load(f):
        out = collections.Counter()
        if not os.path.exists(f): return out
        for l in open(f):
            name, t = l.rstrip('\n').rsplit('\t', 1); out[name.rstrip('0123456789:')] += int(t)
        return out
    a, b = load(d+'/threads0'), load(d+'/threads1')
    return {k: (b[k]-a[k])*10 for k in b if b[k]-a[k] > 0}
def bgs(run):
    out = collections.defaultdict(list)
    for e in run:
        if e['k']=='bg' and 'perf_probe' not in e['name']: out[e['name'].split('/src/')[-1]].append(e['us'])
    return out
def spanmap(run):
    out = collections.defaultdict(list)
    for e in run:
        if e['k']=='span': out[e['name']].append(e['us'])
    return out
def summarize(d):
    ev = [json.loads(l) for l in open(d+'/perf.jsonl')]
    first = min((e['t'] for e in ev if e['k']=='presented'), default=None)
    cpu = open(d+'/cpu').read().strip(); mem = open(d+'/mem').read().split()
    start_drive = None
    try: start_drive = int(open(d+'/drive_start_ms').read())*1000
    except Exception: pass
    lost = sum(e['n'] for e in ev if e['k']=='lost')
    run = [e for e in ev if 't' in e and (start_drive is None or e['t'] >= start_drive - 50_000)]
    pres = sorted(e['t'] for e in run if e['k']=='presented')
    keys = [e for e in run if e['k']=='input' and e['name']=='key_down']
    # A key's frame: the first draw that starts after its handler ends, shown at the first
    # presentation after that draw ends.
    draws_by_start = sorted((e['t'], e['t'] + e['us']) for e in run if e['k']=='draw')
    draw_starts = [d[0] for d in draws_by_start]
    lat = []
    import bisect
    for k in keys:
        j = bisect.bisect_left(draw_starts, k['t'] + k['us'])
        if j == len(draw_starts): continue
        i = bisect.bisect_left(pres, draws_by_start[j][1])
        if i < len(pres): lat.append(pres[i]-k['t'])
    draws = [e for e in run if e['k']=='draw']
    inputs = [e['us'] for e in run if e['k']=='input' and e['name']=='key_down']
    views = collections.defaultdict(list)
    for e in run:
        if e['k']=='view': views[(e['name'].split('::')[-1], e['x']['phase'])].append(e['x']['self_us'])
    tasks = collections.defaultdict(list)
    for e in run:
        if e['k']=='task' and 'perf_probe' not in e['name']: tasks[e['name'].split('/src/')[-1] if '/src/' in e['name'] else e['name']].append(e['us'])
    busy = sum(e['us'] for e in run if e['k'] in ('input','draw','task','action','present')) + sum(e['x'].get('busy_us',0) for e in run if e['k']=='small_polls')
    times = [e['t'] for e in run if 't' in e]
    span = (max(times)-min(times)) if times else 1
    r = {
      'lost_events': lost,
      'first_frame_ms': ms(first) if first else None, 'cpu': cpu, 'rss_kb': mem[1] if len(mem)>1 else None,
      'frames': len(draws), 'keys': len(keys),
      'key_to_present_ms': {'p50': ms(pct(lat,50)), 'p95': ms(pct(lat,95)), 'max': ms(max(lat) if lat else 0)},
      'input_handler_ms': {'p50': ms(pct(inputs,50)), 'p95': ms(pct(inputs,95)), 'max': ms(max(inputs) if inputs else 0)},
      'draw_ms': {'p50': ms(pct([e['us'] for e in draws],50)), 'p95': ms(pct([e['us'] for e in draws],95)), 'max': ms(max([e['us'] for e in draws] or [0]))},
      'draw_phases_p95_ms': {ph: ms(pct([e['x'][ph+'_us'] for e in draws],95)) for ph in ('layout','prepaint','paint')},
      'main_busy_pct': round(100*busy/max(span,1),1),
      'top_views_by_total_ms': sorted([(f'{n}/{ph}', len(v), ms(sum(v)), ms(max(v))) for (n,ph),v in views.items()], key=lambda x:-x[2])[:6],
      'slow_tasks': sorted([(n, len(v), ms(sum(v)), ms(max(v))) for n,v in tasks.items()], key=lambda x:-x[2])[:6],
      'bg_tasks': sorted([(n, len(v), ms(sum(v)), ms(max(v))) for n,v in bgs(run).items()], key=lambda x:-x[2])[:8],
      'spans': sorted([(n, len(v), ms(sum(v)), ms(pct(v,50)), ms(max(v))) for n,v in spanmap(run).items()], key=lambda x:-x[2]),
      'thread_cpu_ms': dict(sorted(threads(d).items(), key=lambda x:-x[1])[:6]),
    }
    return r
for d in sys.argv[1:]:
    if not os.path.exists(d+'/cpu'):
        print('==', os.path.basename(d), 'skipped: the run did not finish'); continue
    print('==', os.path.basename(d)); r=summarize(d)
    if r['lost_events']: print(f" WARNING: the journal lost {r['lost_events']} events; numbers undercount")
    [print(f' {k}: {v}') for k,v in r.items()]

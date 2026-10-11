"""Validate version 1 decoded RTT groups. This measures software HAL completion,
not panel scanout or ECU support. Start a separate capture after every reboot.
"""
import argparse
import json
from pathlib import Path
import re

U32 = (1 << 32) - 1
U64 = (1 << 64) - 1
PIDS = {0, 32, 64, 5, 15, 66, 92}
KINDS = {0: 'frames', 1: 'diagnostics'}
RECORD = re.compile(r'\b(FRAME1|DIAG1|INPUT1|PID1|METRIC1|END1),(.*)$')
LENGTHS = {'FRAME1': 7, 'DIAG1': 5, 'INPUT1': 8, 'PID1': 10, 'METRIC1': 14, 'END1': 2}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def distribution(fields, kind):
    n, overflow, rejected, low, high, *quantiles = fields
    require(n <= U32 and overflow <= n and rejected <= U32, 'invalid histogram counts')
    require(rejected == 0 or n == U32, 'rejection before histogram exhaustion')
    if n == 0:
        require(all(x == 0 for x in fields), 'nonempty statistics with zero samples')
        bounds = [None] * 3
        low = high = None
    else:
        require(low <= high, 'minimum exceeds maximum')
        require(n != 1 or low == high, 'one sample has different extrema')
        bounds = []
        by_rank = {}
        for p, (a, b) in zip((50, 95, 99), zip(quantiles[::2], quantiles[1::2])):
            require(a <= b and max(a, low) <= min(b, high), 'quantile outside observed range')
            rank = (n*p + 99)//100
            require(rank not in by_rank or by_rank[rank] == (a,b), 'equal nearest ranks have different bins')
            by_rank[rank] = (a,b)
            require(rank != 1 or a <= low <= b, 'first rank excludes minimum')
            require(rank != n or a <= high <= b, 'last rank excludes maximum')
            in_overflow = rank > n-overflow
            if kind == 1 and in_overflow:
                require((a, b) == (8192, U64), 'invalid compact overflow bin')
            else:
                width = 1 if a < 32 else 1 << (a.bit_length()-5)
                require(a % width == 0 and b == a+width-1, 'invalid quantile bin')
                if kind == 1:
                    require(b < 8192, 'compact bin out of range')
            bounds.append({'lower': a, 'upper': b})
        require(quantiles[0] <= quantiles[2] <= quantiles[4]
                and quantiles[1] <= quantiles[3] <= quantiles[5], 'unordered quantiles')
        if kind == 1:
            require((overflow > 0) == (high >= 8192), 'overflow disagrees with maximum')
            require((overflow == n) == (low >= 8192), 'overflow disagrees with minimum')
    if kind == 0:
        require(overflow == rejected == 0, 'frame histogram cannot overflow or reject internally')
    return dict(samples=n, overflow=overflow, rejected=rejected, min=low, max=high,
                p50=bounds[0], p95=bounds[1], p99=bounds[2], unit='us' if kind == 0 else 'ms')


def validate(group, previous, prior_frame=False, continuous_from_boot=False):
    kind = group['kind']
    h = group['header']
    rows = group['metrics']
    require(not group['bad'], 'group contains corrupt or duplicate rows')
    require(set(rows) == (set(range(9)) if kind == 0 else PIDS), 'missing or unknown metrics')
    if kind == 0:
        seq, start, end, frames, payload, cycles, rejected = h
        require(end >= start and end-start >= 30_000_000, 'invalid frame window duration')
        require(frames <= U32 and payload % 2 == 0 and 2*frames <= payload <= 153600*frames,
                'invalid completed frame count or payload')
        require(frames != 0 or cycles == 0, 'cycles without a completed frame')
        require(group['inputs'] is not None, 'missing input counters')
        require(all(m['samples'] <= frames for m in rows.values()), 'too many metric samples')
        require(all(rows[i]['samples'] == frames for i in (1, 2, 3, 4)), 'missing per-frame samples')
        require(rows[0]['samples'] == rows[8]['samples'] and rows[6]['samples'] == rows[7]['samples'],
                'mismatched interval/overlap or input batch counts')
        intervals=rows[0]['samples']
        expected=frames if prior_frame else max(0,frames-1)
        if prior_frame or continuous_from_boot:
            require(intervals == expected, 'missing or fabricated preceding frame interval')
        else:
            require(intervals in (frames,max(0,frames-1)), 'too many missing frame intervals')
        if previous:
            require(start >= previous['end_us'], 'overlapping or regressed frame windows')
            if seq == previous['sequence']+1:
                require(start == previous['end_us'], 'contiguous sequence has a time gap')
            require(all(a >= b for a, b in zip(group['inputs'], previous['inputs'])), 'input counters regressed')
        return dict(sequence=seq, start_us=start, end_us=end, frames=frames, payload_bytes=payload,
                    active_cycles=cycles, rejected=rejected, inputs=group['inputs'],
                    metrics=[rows[i] for i in range(9)], accepted_completion_rate_hz=frames*1_000_000/(end-start))
    seq, ecu, epoch, at, published = h
    require(0x7e8 <= ecu <= 0x7ef and epoch != 0 and at <= published, 'invalid ECU, epoch or diagnostic clock')
    require(set(group['pids']) == PIDS, 'missing or unknown PID counters')
    for pid, counters in group['pids'].items():
        require(all(c <= U32 for c in counters), 'PID counter out of range')
        requested, sent, completed, negative, timeouts, malformed, abandoned, faults = counters
        require(sent <= requested and faults <= abandoned, 'impossible PID counts')
        replies=completed+negative
        require(min(completed+negative+timeouts+malformed+abandoned,U32) <= requested, 'outcomes exceed requests')
        require(min(replies,U32) <= sent, 'reply without a transmitted request')
        require(rows[pid]['samples'] == min(replies,U32), 'terminal reply and RTT counts disagree')
        # Individual outcome counters saturate independently. Their sum is a
        # lower bound after either saturates; histogram samples freeze at U32.
        rejected=rows[pid]['rejected']
        require(rejected >= max(0,replies-U32), 'missing exhausted-histogram rejections')
        if completed != U32 and negative != U32:
            require(rejected == max(0,replies-U32), 'unexpected histogram rejections')
        if previous:
            old = previous['pids'][pid]
            require(all(a >= b for a, b in zip(counters, old)), 'PID counters regressed')
            old_d=previous['metrics'][pid]
            new_d=rows[pid]
            require(all(new_d[k] >= old_d[k] for k in ('samples','overflow','rejected')), 'RTT counts regressed')
            require(new_d['overflow']-old_d['overflow'] <= new_d['samples']-old_d['samples'], 'too many new overflow samples')
            if old_d['samples']:
                require(new_d['min'] <= old_d['min'] and new_d['max'] >= old_d['max'], 'retained extrema contracted')
            if new_d['samples'] == old_d['samples']:
                require(all(new_d[k] == old_d[k] for k in ('min','max','overflow','p50','p95','p99')), 'histogram changed without samples')
    if previous:
        require(ecu == previous['ecu'] and epoch >= previous['epoch'] and at >= previous['session_ms']
                and published >= previous['published_ms'], 'diagnostic identity or clock regressed')
    return dict(sequence=seq, ecu=ecu, epoch=epoch, session_ms=at, published_ms=published,
                pids=group['pids'], metrics=rows)


def parse(lines):
    result = dict(frames=[], diagnostics=[], errors=[], missing_or_incomplete=dict(frames=0, diagnostics=0),
                  scope='Software observations only. Split captures at reboot; no physical FPS or ECU qualification.')
    active = {}
    seen = {0: 0, 1: 0}
    last = {0: None, 1: None}
    prior_frame = False
    for number, line in enumerate(lines, 1):
        match = RECORD.search(line.rstrip('\r\n'))
        if not match:
            continue
        name, data = match.groups()
        try:
            require(len(data) <= 4096, 'record too long')
            tokens = data.split(',')
            require(len(tokens) == LENGTHS[name] and all(re.fullmatch(r'[0-9]{1,20}', x) for x in tokens),
                    'invalid integer record')
            values = [int(x) for x in tokens]
            require(all(x <= U64 for x in values), 'integer exceeds u64')
            if name in ('FRAME1', 'DIAG1'):
                kind = int(name == 'DIAG1')
                seq = values[0]
                if kind in active:
                    result['errors'].append(dict(line=number, reason=f'incomplete {KINDS[kind]} group {active[kind]["header"][0]}'))
                    del active[kind]
                require(seq > seen[kind], 'replayed/reset header; split capture after reboot')
                seen[kind] = seq
                active[kind] = dict(kind=kind, header=values, inputs=None, metrics={}, pids={}, bad=False)
                continue
            if name in ('METRIC1', 'END1'):
                kind, seq, *rest = values
            else:
                kind = int(name == 'PID1')
                seq, *rest = values
            require(kind in KINDS, 'unknown group kind')
            require(kind in active and active[kind]['header'][0] == seq, 'orphan or foreign sequence')
            group = active[kind]
            if name == 'END1':
                del active[kind]
                value = validate(group, last[kind], prior_frame, seq == len(result['frames'])+1)
                result[KINDS[kind]].append(value)
                last[kind] = value
                if kind == 0:
                    prior_frame = prior_frame or value['frames'] != 0
            elif name == 'INPUT1':
                require(group['inputs'] is None, 'duplicate input counters')
                group['inputs'] = rest
            else:
                key, *fields = rest
                destination = group['metrics'] if name == 'METRIC1' else group['pids']
                require(key not in destination, 'duplicate metric or PID')
                destination[key] = distribution(fields, kind) if name == 'METRIC1' else fields
        except ValueError as error:
            # A corrupted identity cannot reliably identify its owning group.
            # Conservatively invalidate all currently open groups.
            for group in active.values():
                group['bad'] = True
            result['errors'].append(dict(line=number, reason=str(error)))
    for kind, group in active.items():
        result['errors'].append(dict(line=None, reason=f'incomplete {KINDS[kind]} group {group["header"][0]} at EOF'))
    for kind, name in KINDS.items():
        result['missing_or_incomplete'][name] = seen[kind]-len(result[name])
    if not result['frames'] and not result['diagnostics']:
        result['errors'].append(dict(line=None, reason='no complete recognized telemetry groups'))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(args.capture.resolve() != args.output.resolve(), 'output must not overwrite capture')
    with args.capture.open(encoding='utf-8', errors='strict') as stream:
        result = parse(stream)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(f"Accepted {len(result['frames'])} frame windows and {len(result['diagnostics'])} diagnostic snapshots; {len(result['errors'])} errors")
    return int(bool(result['errors']))


if __name__ == '__main__':
    raise SystemExit(main())

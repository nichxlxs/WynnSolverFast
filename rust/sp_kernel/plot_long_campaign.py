#!/usr/bin/env python3
"""Plot common frozen-score milestones; never imply a certified optimum."""
import argparse
import json
from pathlib import Path
import statistics

from benchmark_quality import score_at


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--campaign', type=Path, required=True)
    ap.add_argument('--references', type=Path, required=True)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt

    data = json.loads(args.campaign.read_text())
    refs = json.loads(args.references.read_text())
    queries = [
        ('fam_spellsteal_medium', 'Oblivion spellsteal · 5 open slots'),
        ('meta_mage_arcanist_meteor_remove_6', 'Arcanist · 6 open slots'),
        ('meta_mage_light_bender_healing_remove_6', 'Light Bender · 6 open slots'),
    ]
    arms = [('current_exact15', 'Enumeration', '#68778d'),
            ('current_alns15', 'Current heuristic', '#087e8b'),
            ('elite_alns15', 'Six-slot repair experiment', '#a13f9d')]
    plt.rcParams.update({'font.size': 10, 'axes.spines.top': False,
                         'axes.spines.right': False, 'svg.fonttype': 'none'})
    fig, axes = plt.subplots(1, 3, figsize=(12.6, 4.2), constrained_layout=True)
    for ax, (query, title) in zip(axes, queries):
        reference = refs[query]['score']
        for arm, label, color in arms:
            runs = [r for r in data['runs'] if r['scenario'] == query
                    and r['variant'] == arm and r['status'] not in ('failed', 'unavailable_fixture')]
            expected = 1 if arm == 'current_exact15' else 2
            if len(runs) != expected:
                raise ValueError(f'{query}/{arm}: expected {expected} valid runs, found {len(runs)}')
            stop = min(r['budget_seconds'] for r in runs)
            times = sorted({stop, *(e['observed_seconds'] for r in runs for e in r['trajectory']
                                   if 0 < e['observed_seconds'] <= stop)})
            points = []
            for t in times:
                scores = [score_at(r['trajectory'], t) for r in runs]
                if all(s is not None for s in scores):
                    points.append((t, 100 * statistics.median(scores) / reference))
            if points:
                x, y = zip(*points)
                ax.step(x, y, where='post', color=color, linewidth=2, label=label)
                ax.scatter([x[-1]], [y[-1]], color=color, marker='|', s=110, zorder=3)
        ax.axhline(99, color='#222222', linestyle='--', linewidth=1, label='Frozen 99% milestone')
        ax.set_xscale('log')
        ax.set_xlim(.03, 200)
        ax.set_xticks([.1, 1, 10, 30, 60, 180], ['0.1', '1', '10', '30', '60', '180'])
        ax.set_title(title, loc='left', fontweight='bold')
        ax.set_xlabel('Elapsed wall time (seconds, log scale)')
        ax.grid(axis='y', color='#e3e7ed', linewidth=.7)
    axes[0].set_ylabel('Score / frozen best-known reference (%)')
    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='outside lower center', ncol=4, frameon=False)
    fig.suptitle('Long searches: time to the same strong score', x=.02, ha='left', fontsize=15, fontweight='bold')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.out, dpi=170, bbox_inches='tight', facecolor='white')
    plt.close(fig)


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Waits for a quiet machine before a run of Gate P or Gate I (ADR-0287).

The machine is quiet once no 10 s of the last 300 s had its threads busy past
0.4, read from /proc/stat's busy ticks as the run's own sampler reads them
(scripts/profile/load.py, ADR-0235). The run's sampler still decides whether
other work shared the run; the wait spares an attempt begun beside work that
the 1-minute load average does not show, a 10 s burst moving it little.

Usage: scripts/profile/quiet.py [--wait <seconds>]
Prints how long it waited and the machine's busiest 10 s, and exits 0 once
quiet, or 1 where no quiet came within the wait or /proc/stat cannot be read.
"""

import argparse
import collections
import os
import sys
import time

WINDOW = 300  # the seconds sampled, each of their 10 s windows to be quiet (ADR-0287)
BURST = 10    # seconds, as long as the window the run's sampler judges (ADR-0235 decision 3)
QUIET = 0.4   # threads, below the run's bound on other work, 0.5 (load.py's OTHER_MAX)


def busy_ticks():
    """The machine's busy ticks so far: user, nice, system, irq, softirq and steal, as load.py counts them."""
    with open("/proc/stat") as f:
        cpu = f.readline().split()
    return sum(int(cpu[i]) for i in (1, 2, 3, 6, 7, 8))


def busiest(samples, tick):
    """The threads busy in the busiest BURST samples, each sample (seconds, ticks), weighed by its length."""
    w = min(BURST, len(samples))
    return max(sum(t for _, t in samples[i:i + w]) / tick / sum(s for s, _ in samples[i:i + w])
               for i in range(len(samples) - w + 1))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--wait", type=int, default=10800, help="the longest wait, in seconds")
    a = ap.parse_args()
    try:
        tick = os.sysconf("SC_CLK_TCK")
        last, at = busy_ticks(), time.monotonic()
    except (OSError, ValueError) as e:
        print(f"no quiet machine told: /proc/stat not read ({e})")
        return 1
    start = at
    samples = collections.deque(maxlen=WINDOW)
    while True:
        time.sleep(1)
        now, t = busy_ticks(), time.monotonic()
        samples.append((t - at, now - last))
        last, at = now, t
        waited = t - start
        if len(samples) == WINDOW and (b := busiest(list(samples), tick)) < QUIET:
            print(f"quiet after {waited:.0f} s: the machine's busiest {BURST} s of the last {WINDOW} s used {b:.2f} threads")
            return 0
        if waited >= a.wait:
            b = busiest(list(samples), tick)
            print(f"no quiet machine in {a.wait} s: the machine's busiest {BURST} s of the last {len(samples)} s used {b:.2f} threads")
            return 1


if __name__ == "__main__":
    sys.exit(main())

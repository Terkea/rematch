"""EXP-0030 feasibility check, run once before registration (preregistration.md, section 7).

A scratch script, not the analysis: it computes the memory interval in Python on EXP-0015's
world (its analysis.py) on the development namespace EXP-0015-dev, to see whether the candidates
were worth registering. Kept for the record; nothing reads it.

Run in the lab image with /repo the checkout: python dev/feasibility.py
"""
import sys, math, numpy as np
sys.path.insert(0, "/repo/lab/experiments/EXP-0015")
import analysis as A
from rematch_lab.experiments import seed
Q = math.log(10)/400
def within(m, phi):
    if m == 0: return 0.0
    return m*(1+phi)/(1-phi) - 2*phi*(1-phi**m)/(1-phi)**2
def cross(a0,a1,b0,b1,phi):
    sa=(1-phi**(a1-a0))/(1-phi); sb=(1-phi**(b1-b0))/(1-phi)
    return phi**(b0-(a1-1))*sa*sb
def var_diff(a0,a1,b0,b1,s2,phi):
    na, nb = a1-a0, b1-b0
    return s2*(within(na,phi)/na**2 + within(nb,phi)/nb**2 - 2*cross(a0,a1,b0,b1,phi)/(na*nb))
def verdict(r, mode, z=1.2815516):
    r = np.asarray(r, float); n = len(r); W = min(30, n//2)
    d = np.diff(r); rms = math.sqrt(np.mean(d*d))
    s2 = rms/Q; a = Q*rms/2; phi = min(max(1-a, 0.0), 1-1e-9)
    s2 = s2/(1-a/2)
    if mode == "halves": a0,a1 = 0,W
    else: a0,a1 = 0,n-W
    b0,b1 = n-W,n
    diff = r[b0:b1].mean()-r[a0:a1].mean()
    sd = math.sqrt(var_diff(a0,a1,b0,b1,s2,phi))
    return 1 if diff - z*sd > 0 else (-1 if diff + z*sd < 0 else 0)
def sim(name, players, games, decline, system):
    rng = np.random.default_rng(seed("EXP-0015-dev", name)); out=[]
    for _ in range(players):
        t0 = A.strength(rng); truth = np.full(300+games, t0)
        if decline: truth[300:] = t0 - decline*np.arange(games)/(games-1)
        out.append([g["rating"] for g in A.play(rng, truth, 300, system)])
    return out
for sysname, system in [("glicko2",("glicko2",0.0)),("elo32",("elo",32.0)),("elo10",("elo",10.0))]:
    S = sim("settled"+sysname, 300, 250, 0, system)
    for g in (60,120,250):
        for mode in ("halves","before"):
            v = [verdict(s[:g], mode) for s in S]
            print(sysname, g, mode, "fall", sum(x==-1 for x in v)/len(v), "rise", sum(x==1 for x in v)/len(v))
for g in (60,120,250):
    D = sim(f"decline/200/{g}", 200, g, 200, ("glicko2",0.0))
    for mode in ("halves","before"):
        v=[verdict(s, mode) for s in D]; print("det200", g, mode, sum(x==-1 for x in v)/len(v))

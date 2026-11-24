# EXP-0007 pilot: disagreements for the hand check

Judged by hand from the position and the solution, before the confirmatory run (preregistration.md, section 7). The pilot is development data. A tag is label-limited when at least half of its checked "tagged, not labelled" puzzles are label gaps.

| Tag | Tagged, not labelled: label gaps / checked | Label-limited | Labelled, not tagged: what we missed |
|---|---|---|---|
| hangingPiece | 3 / 4 (1 tagger error) | yes | none in the pilot |
| fork | 2 / 2 | yes | 1 truncated line, 1 unclear label |
| pin | 3 / 4 (1 tagger error) | yes | 1 by definition (a pin that stops a recapture), 1 window |
| skewer | 3 / 4 (1 tagger error) | yes | 2 window |
| discoveredAttack | none in the pilot | no | 1 horizon, 1 line opened by the recapture |
| deflection | 0 / 4 (3 tagger errors: attraction; 1 unclear) | no | 2 rule shape (a king driven off by check; a win two moves later) |
| sacrifice | 4 / 4 | yes | 2 (a gain first; window) |
| promotion | none in the pilot | no | 2 no swing |
| backRankMate | 4 / 4 | yes | 2 label errors (no mate in the solution) |
| mate, mateIn1 to mateIn4Plus, smotheredMate | no disagreement in the pilot | no | none |

What the misses have in common: the rules read only the window up to the swing's depth plus one, and the swing settles at the first gain, so a motif after an early capture is not seen (pin, skewer, sacrifice); a line whose gain comes after ply 8 has no swing and no tags at all.

## hangingPiece: tagged, not labelled: Gwk3I

- Lichess themes: backRankMate mate mateIn1 middlegame oneMove
- Our tags: backRankMate hangingPiece mate mateIn1
- Position (to move: b): `r5k1/p2b1rpp/4n3/3Q4/2p1R3/8/PPP3PP/5R1K b - - 0 29`
- Solution: f7f1
- Verdict: tagger error. Rf7xf1 is mate; the rook is defended only by its king, which the rule allows, but the motif is the back-rank mate, not a loose piece.

```
8 r . . . . . k .
7 p . . b . r p p
6 . . . . n . . .
5 . . . Q . . . .
4 . . p . R . . .
3 . . . . . . . .
2 P P P . . . P P
1 . . . . . R . K
  a b c d e f g h
```

## hangingPiece: tagged, not labelled: HPlia

- Lichess themes: crushing endgame long pin
- Our tags: hangingPiece
- Position (to move: w): `4r1k1/5pbp/3p2p1/3P4/1P1p4/P6P/2qB1PP1/4R1K1 w - - 0 29`
- Solution: e1e8 g7f8 d2h6 c2c1 h6c1
- Verdict: label gap. Re1xe8+ takes a rook nothing defends (Bg7-f8 only blocks); the pin that follows is Lichess's theme.

```
8 . . . . r . k .
7 . . . . . p b p
6 . . . p . . p .
5 . . . P . . . .
4 . P . p . . . .
3 P . . . . . . P
2 . . q B . P P .
1 . . . . R . K .
  a b c d e f g h
```

## hangingPiece: tagged, not labelled: PwLEk

- Lichess themes: crushing endgame long
- Our tags: hangingPiece
- Position (to move: w): `8/5p2/5k1p/2p2Pp1/1pn1K3/3P3P/1P3P2/8 w - - 0 36`
- Solution: d3c4 b4b3 e4d5 f6f5 d5c5
- Verdict: label gap. dxc4 takes a knight nothing defends.

```
8 . . . . . . . .
7 . . . . . p . .
6 . . . . . k . p
5 . . p . . P p .
4 . p n . K . . .
3 . . . P . . . P
2 . P . . . P . .
1 . . . . . . . .
  a b c d e f g h
```

## hangingPiece: tagged, not labelled: x8SGJ

- Lichess themes: mate mateIn2 middlegame short
- Our tags: hangingPiece mate mateIn2 matingNet
- Position (to move: b): `3r3k/p4B2/1p4RP/5p2/3ppP2/q2nP3/P2P2Q1/2R2K2 b - - 0 36`
- Solution: a3c1 f1e2 c1e1
- Verdict: label gap. Qxc1+ takes a rook nothing defends, and the mate follows.

```
8 . . . r . . . k
7 p . . . . B . .
6 . p . . . . R P
5 . . . . . p . .
4 . . . p p P . .
3 q . . n P . . .
2 P . . P . . Q .
1 . . R . . K . .
  a b c d e f g h
```

## fork: tagged, not labelled: egHtH

- Lichess themes: advantage interference middlegame short
- Our tags: fork
- Position (to move: w): `2r2rk1/1p3pb1/pBnp2p1/3Np3/4n2q/1N6/PPP1Q2P/1K1R2R1 w - - 4 20`
- Solution: g1g4 h4h6 g4e4
- Verdict: label gap. Rg4 attacks the queen on h4 and the knight on e4 and is defended by Qe2; after Qh6, Rxe4.

```
8 . . r . . r k .
7 . p . . . p b .
6 p B n p . . p .
5 . . . N p . . .
4 . . . . n . . q
3 . N . . . . . .
2 P P P . Q . . P
1 . K . R . . R .
  a b c d e f g h
```

## fork: tagged, not labelled: gKZfe

- Lichess themes: advancedPawn crushing endgame long promotion
- Our tags: fork
- Position (to move: w): `r5k1/3PR3/4P3/8/2b2KP1/8/1p6/8 w - - 0 59`
- Solution: e7e8 g8h7 e8a8 b2b1q d7d8q
- Verdict: label gap. Re8+ attacks the king on g8 and the rook on a8 along the eighth rank; Kh7, Rxa8.

```
8 r . . . . . k .
7 . . . P R . . .
6 . . . . P . . .
5 . . . . . . . .
4 . . b . . K P .
3 . . . . . . . .
2 . p . . . . . .
1 . . . . . . . .
  a b c d e f g h
```

## fork: labelled, not tagged: ViBG8

- Lichess themes: crushing discoveredAttack fork long middlegame
- Our tags: discoveredAttack sacrifice
- Position (to move: b): `2rqr1k1/pp1N1ppp/5n2/2bp1b2/8/1P1P2P1/PBQNPPBP/R4RK1 b - - 0 13`
- Solution: c5f2 f1f2 c8c2 d7f6 g7f6
- Verdict: unclear. No fork in the solution: Bxf2+ discovers Rc8 on the queen (our discoveredAttack and sacrifice are right); Lichess's fork looks wrong.

```
8 . . r q r . k .
7 p p . N . p p p
6 . . . . . n . .
5 . . b p . b . .
4 . . . . . . . .
3 . P . P . . P .
2 P B Q N P P B P
1 R . . . . R K .
  a b c d e f g h
```

## fork: labelled, not tagged: rSXRE

- Lichess themes: advantage defensiveMove fork middlegame short
- Our tags: (none)
- Position (to move: b): `3r1rk1/pR3pp1/1b2n2p/3B4/8/1Q4PP/PP3P2/6K1 b - - 2 28`
- Solution: e6c5 d5f7 g8h8
- Verdict: tagger miss, truncated line. Nc5 forks Qb3 and Rb7, but the solution stops at Bxf7+ Kh8, so the completing capture the rule needs never comes.

```
8 . . . r . r k .
7 p R . . . p p .
6 . b . . n . . p
5 . . . B . . . .
4 . . . . . . . .
3 . Q . . . . P P
2 P P . . . P . .
1 . . . . . . K .
  a b c d e f g h
```

## pin: tagged, not labelled: YXebm

- Lichess themes: mate mateIn2 middlegame queensideAttack short
- Our tags: mate mateIn2 matingNet pin
- Position (to move: b): `r4rk1/p5pp/2ppb3/q3b3/4P3/1PN1B1PB/1PPQ1P2/2KR3R b - - 0 21`
- Solution: a5a1 c3b1 a1b2
- Verdict: tagger error. Qa1+ Nb1 Qxb2# takes an unpinned pawn; no pin in the solution.

```
8 r . . . . r k .
7 p . . . . . p p
6 . . p p b . . .
5 q . . . b . . .
4 . . . . P . . .
3 . P N . B . P B
2 . P P Q . P . .
1 . . K R . . . R
  a b c d e f g h
```

## pin: tagged, not labelled: grnPc

- Lichess themes: deflection endgame fork long mate mateIn3
- Our tags: backRankMate mate mateIn3 matingNet pin
- Position (to move: b): `6k1/4q1p1/p1Qp1b1p/1p1P4/8/1P3P2/P2N2PP/6K1 b - - 0 28`
- Solution: e7e1 d2f1 f6d4 g1h1 e1f1
- Verdict: label gap. After Qe1+ Nf1 the knight is pinned to the king on g1, and Qxf1# takes it.

```
8 . . . . . . k .
7 . . . . q . p .
6 p . Q p . b . p
5 . p . P . . . .
4 . . . . . . . .
3 . P . . . P . .
2 P . . N . . P P
1 . . . . . . K .
  a b c d e f g h
```

## pin: tagged, not labelled: s45E0

- Lichess themes: advantage kingsideAttack long middlegame
- Our tags: pin
- Position (to move: w): `3r2rk/6qp/2pb4/5p2/Pp2Pp2/1P3P2/1Q1R1P1P/3R3K w - - 0 28`
- Solution: b2g7 h8g7 d2d6 d8d6 d1d6
- Verdict: label gap. After the queen trade the bishop on d6 is pinned against the rook on d8 and falls (Rxd6 Rxd6 Rxd6).

```
8 . . . r . . r k
7 . . . . . . q p
6 . . p b . . . .
5 . . . . . p . .
4 P p . . P p . .
3 . P . . . P . .
2 . Q . R . P . P
1 . . . R . . . K
  a b c d e f g h
```

## pin: tagged, not labelled: xe73w

- Lichess themes: attraction deflection exposedKing master mate mateIn4 middlegame veryLong
- Our tags: deflection mate mateIn4Plus matingNet pin sacrifice
- Position (to move: b): `5r1k/p5bp/4R3/7q/3Pn1N1/4P3/1PP1Q1PB/4K3 b - - 2 30`
- Solution: h5a5 c2c3 a5a1 e2d1 f8f1 e1f1 a1d1
- Verdict: label gap. Qd1 blocks the check and is pinned by Qa1 to the king; Rf1+ Kxf1 Qxd1#.

```
8 . . . . . r . k
7 p . . . . . b p
6 . . . . R . . .
5 . . . . . . . q
4 . . . P n . N .
3 . . . . P . . .
2 . P P . Q . P B
1 . . . . K . . .
  a b c d e f g h
```

## pin: labelled, not tagged: 5UyIN

- Lichess themes: mate mateIn1 oneMove opening pin smotheredMate
- Our tags: mate mateIn1 smotheredMate
- Position (to move: b): `r3kbnr/ppp1qppp/8/4n3/1PPp2b1/P4N2/1B1NPPPP/R2QKB1R b KQkq - 1 8`
- Solution: e5d3
- Verdict: tagger miss by definition. The e2 pawn is pinned, so exd3 is illegal and Nd3 mates; our pin rule only counts a pinned piece that is captured.

```
8 r . . . k b n r
7 p p p . q p p p
6 . . . . . . . .
5 . . . . n . . .
4 . P P p . . b .
3 P . . . . N . .
2 . B . N P P P P
1 R . . Q K B . R
  a b c d e f g h
```

## pin: labelled, not tagged: auo2d

- Lichess themes: advantage clearance hangingPiece middlegame pin quietMove veryLong
- Our tags: hangingPiece
- Position (to move: b): `r2qr2k/ppp3pp/2p2p2/4B3/8/3Q3P/PPP2PP1/R3KB1R b KQ - 3 15`
- Solution: e8e5 f1e2 d8d3 c2d3 a8e8 e1c1 e5e2
- Verdict: tagger miss, window. Rxe5 wins at once, so the window ends after two plies, before Be2 is pinned and taken on ply 7.

```
8 r . . q r . . k
7 p p p . . . p p
6 . . p . . p . .
5 . . . . B . . .
4 . . . . . . . .
3 . . . Q . . . P
2 P P P . . P P .
1 R . . . K B . R
  a b c d e f g h
```

## skewer: tagged, not labelled: 0uCYP

- Lichess themes: backRankMate fork long mate mateIn3 middlegame sacrifice
- Our tags: backRankMate fork mate mateIn3 matingNet sacrifice skewer
- Position (to move: b): `5k1r/p2Q2p1/3p1q1p/2bR4/4r3/5N2/PP3PPP/R1B3K1 b - - 0 19`
- Solution: e4e1 f3e1 f6f2 g1h1 f2e1
- Verdict: tagger error. Qxf2+ passes f3, which the knight left for Nxe1, but that knight was deflected, not skewered.

```
8 . . . . . k . r
7 p . . Q . . p .
6 . . . p . q . p
5 . . b R . . . .
4 . . . . r . . .
3 . . . . . N . .
2 P P . . . P P P
1 R . B . . . K .
  a b c d e f g h
```

## skewer: tagged, not labelled: AkJTy

- Lichess themes: crushing endgame rookEndgame short
- Our tags: skewer
- Position (to move: w): `8/2R5/5kr1/1p1p3K/2p5/P7/1PP5/8 w - - 0 43`
- Solution: c7c6 f6f5 c6g6
- Verdict: label gap. Rc6+ checks the king along the sixth rank; Kf5, Rxg6: a textbook skewer.

```
8 . . . . . . . .
7 . . R . . . . .
6 . . . . . k r .
5 . p . p . . . K
4 . . p . . . . .
3 P . . . . . . .
2 . P P . . . . .
1 . . . . . . . .
  a b c d e f g h
```

## skewer: tagged, not labelled: MEZvP

- Lichess themes: attraction crushing endgame exposedKing fork veryLong
- Our tags: fork skewer
- Position (to move: w): `4r3/p1p1k1p1/1p6/1P4Pn/2B2R2/1P6/3r4/5RK1 w - - 1 27`
- Solution: f4e4 e7d8 e4e8 d8e8 c4f7 e8e7 f7h5
- Verdict: label gap. Re4+ Kd8 Rxe8+ skewers the king and the rook on the e-file.

```
8 . . . . r . . .
7 p . p . k . p .
6 . p . . . . . .
5 . P . . . . P n
4 . . B . . R . .
3 . P . . . . . .
2 . . . r . . . .
1 . . . . . R K .
  a b c d e f g h
```

## skewer: tagged, not labelled: MjShB

- Lichess themes: deflection discoveredAttack discoveredCheck doubleCheck interference mate mateIn2 middlegame short
- Our tags: discoveredAttack mate mateIn2 matingNet skewer
- Position (to move: b): `5Nk1/pq2n3/Pp1R2p1/3nPp1p/3P4/6P1/1PrB1PK1/4Q2R b - - 0 31`
- Solution: d5f4 g2f1 b7h1
- Verdict: label gap. The king on g2 stands on the long diagonal in front of the rook on h1; the double check drives it off and Qxh1# takes the rook behind it.

```
8 . . . . . N k .
7 p q . . n . . .
6 P p . R . . p .
5 . . . n P p . p
4 . . . P . . . .
3 . . . . . . P .
2 . P r B . P K .
1 . . . . Q . . R
  a b c d e f g h
```

## skewer: labelled, not tagged: MaWl7

- Lichess themes: capturingDefender crushing middlegame queensideAttack skewer veryLong
- Our tags: (none)
- Position (to move: b): `1r3rk1/p2P1ppp/5b2/q1pQ4/5B2/2P5/PP3PPP/2KR3R b - - 0 17`
- Solution: b8b2 d1d2 a5c3 c1d1 b2b1 d1e2 b1h1
- Verdict: tagger miss, window. The skewer Rb1+ Ke2 Rxh1 comes on plies 5 to 7, after the window ends.

```
8 . r . . . r k .
7 p . . P . p p p
6 . . . . . b . .
5 q . p Q . . . .
4 . . . . . B . .
3 . . P . . . . .
2 P P . . . P P P
1 . . K R . . . R
  a b c d e f g h
```

## skewer: labelled, not tagged: kouit

- Lichess themes: crushing endgame skewer veryLong
- Our tags: (none)
- Position (to move: b): `8/4Q1p1/5pkp/1bP5/1P4PP/2B2P2/6K1/3q4 b - - 2 48`
- Solution: d1f1 g2g3 f1g1 g3f4 g1h2 f4e4 h2e2 e4d5 e2e7
- Verdict: tagger miss, window. The skewer Qe2+ Kd5 Qxe7 comes on plies 7 to 9.

```
8 . . . . . . . .
7 . . . . Q . p .
6 . . . . . p k p
5 . b P . . . . .
4 . P . . . . P P
3 . . B . . P . .
2 . . . . . . K .
1 . . . q . . . .
  a b c d e f g h
```

## discoveredAttack: labelled, not tagged: gU9r4

- Lichess themes: attraction crushing deflection discoveredCheck doubleCheck kingsideAttack middlegame sacrifice skewer veryLong
- Our tags: (none)
- Position (to move: b): `1R3bk1/2N1pp1p/q2p2pB/2pP4/2n1P3/8/2Q2PPP/r4NK1 b - - 6 29`
- Solution: a1f1 g1f1 c4e3 f1e1 a6f1 e1d2 f1f2 d2d3 f2c2 d3e3 c2c1
- Verdict: tagger miss, horizon. The discovered double check Ne3+ is on ply 3, but the line wins its material after ply 8, so no swing is found within LINE_MAX_PLIES and nothing is tagged.

```
8 . R . . . b k .
7 . . N . p p . p
6 q . . p . . p B
5 . . p P . . . .
4 . . n . P . . .
3 . . . . . . . .
2 . . Q . . P P P
1 r . . . . N K .
  a b c d e f g h
```

## discoveredAttack: labelled, not tagged: ibd64

- Lichess themes: advantage discoveredAttack middlegame short
- Our tags: sacrifice
- Position (to move: w): `6k1/pp2q2p/2pp2p1/2nP3n/2P1p3/2N1Qr1N/PP4rP/3RR2K w - - 2 25`
- Solution: e3f3 e4f3 e1e7
- Verdict: tagger miss. Qxf3 exf3 Rxe7: the e-file opens only when the e4 pawn recaptures, and the rule checks the attack right after the vacating move.

```
8 . . . . . . k .
7 p p . . q . . p
6 . . p p . . p .
5 . . n P . . . n
4 . . P . p . . .
3 . . N . Q r . N
2 P P . . . . r P
1 . . . R R . . K
  a b c d e f g h
```

## deflection: tagged, not labelled: BURkp

- Lichess themes: advantage discoveredAttack intermezzo middlegame veryLong
- Our tags: deflection discoveredAttack sacrifice
- Position (to move: b): `2r1r1k1/pp3ppp/1qn2n2/3p4/3N4/1N2P3/PP2QPPP/2R2RK1 b - - 2 16`
- Solution: c6d4 b3d4 c8c1 f1c1 b6d4 e3d4 e8e2
- Verdict: unclear. Qxd4 exd4 draws the e3 pawn off the e-file so Re8 hits Qe2: a clearance by attraction more than a deflection.

```
8 . . r . r . k .
7 p p . . . p p p
6 . q n . . n . .
5 . . . p . . . .
4 . . . N . . . .
3 . N . . P . . .
2 P P . . Q P P P
1 . . R . . R K .
  a b c d e f g h
```

## deflection: tagged, not labelled: NwCoZ

- Lichess themes: advantage attraction fork long middlegame sacrifice
- Our tags: deflection fork sacrifice
- Position (to move: b): `1rq1r1k1/3bppb1/3p2p1/1p1Nn2p/1P2P3/3PB1PP/3Q1PB1/2R1K2R b K - 2 19`
- Solution: c8c1 d2c1 e5d3 e1e2 d3c1
- Verdict: tagger error. Qxc1+ Qxc1 Nd3+ is attraction (the queen is drawn to c1 for the fork), not deflection.

```
8 . r q . r . k .
7 . . . b p p b .
6 . . . p . . p .
5 . p . N n . . p
4 . P . . P . . .
3 . . . P B . P P
2 . . . Q . P B .
1 . . R . K . . R
  a b c d e f g h
```

## deflection: tagged, not labelled: aXygm

- Lichess themes: attraction discoveredAttack endgame mate mateIn4 sacrifice veryLong
- Our tags: deflection discoveredAttack mate mateIn4Plus matingNet sacrifice
- Position (to move: b): `2k4r/pp6/7r/4P3/4Rpp1/2PQ2Pq/P5RP/6K1 b - - 0 27`
- Solution: h3g2 g1g2 h6h2 g2f1 h2h1 f1e2 h8h2
- Verdict: tagger error. Qxg2+ removes the rook that guards h2 (capturing the defender, attraction of the king); nothing is deflected.

```
8 . . k . . . . r
7 p p . . . . . .
6 . . . . . . . r
5 . . . . P . . .
4 . . . . R p p .
3 . . P Q . . P q
2 P . . . . . R P
1 . . . . . . K .
  a b c d e f g h
```

## deflection: tagged, not labelled: n19fI

- Lichess themes: attraction discoveredAttack endgame long mate mateIn3 sacrifice
- Our tags: deflection discoveredAttack mate mateIn3 matingNet sacrifice
- Position (to move: b): `8/p4k2/3q1pr1/3p1Q2/3P4/6P1/PPr4P/1R3RK1 b - - 3 33`
- Solution: g6g3 g1h1 g3g1 h1g1 d6h2
- Verdict: tagger error. Rg1+ Kxg1 Qh2# draws the king in (attraction); nothing is deflected.

```
8 . . . . . . . .
7 p . . . . k . .
6 . . . q . p r .
5 . . . p . Q . .
4 . . . P . . . .
3 . . . . . . P .
2 P P r . . . . P
1 . R . . . R K .
  a b c d e f g h
```

## deflection: labelled, not tagged: 4uKvP

- Lichess themes: crushing deflection long middlegame sacrifice
- Our tags: (none)
- Position (to move: w): `3n4/p3kp2/3qpb2/3p1Q2/P1pP3r/2P1P3/1R3PP1/1R4K1 w - - 0 32`
- Solution: b2b7 d8b7 b1b7 e7e8 f5f6
- Verdict: tagger miss. Rxb7+ drives the king off e7, which defended f6, and Qxf6 follows; the rule needs the reply to recapture or check, and a king that steps away does neither.

```
8 . . . n . . . .
7 p . . . k p . .
6 . . . q p b . .
5 . . . p . Q . .
4 P . p P . . . r
3 . . P . P . . .
2 . R . . . P P .
1 . R . . . . K .
  a b c d e f g h
```

## deflection: labelled, not tagged: eE9oz

- Lichess themes: advantage attraction clearance deflection endgame long sacrifice
- Our tags: sacrifice
- Position (to move: w): `6k1/1b1Rrqpp/p7/1p6/5P2/2P4Q/PKP3PP/8 w - - 3 26`
- Solution: d7b7 e7b7 h3c8 f7f8 c8b7
- Verdict: tagger miss. The win comes with Qc8+ Qf8 Qxb7, two moves after the deflection, not on the next capture.

```
8 . . . . . . k .
7 . b . R r q p p
6 p . . . . . . .
5 . p . . . . . .
4 . . . . . P . .
3 . . P . . . . Q
2 P K P . . . P P
1 . . . . . . . .
  a b c d e f g h
```

## sacrifice: tagged, not labelled: CNSE4

- Lichess themes: advancedPawn crushing deflection endgame promotion short
- Our tags: promotion sacrifice
- Position (to move: b): `1r6/bP3R2/8/B7/4k3/6Pp/5p1P/3K4 b - - 5 53`
- Solution: b8b7 f7b7 f2f1q
- Verdict: label gap. Rxb7 Rxb7 gives a rook for a pawn so f1=Q comes with check: a sacrifice.

```
8 . r . . . . . .
7 b P . . . R . .
6 . . . . . . . .
5 B . . . . . . .
4 . . . . k . . .
3 . . . . . . P p
2 . . . . . p . P
1 . . . K . . . .
  a b c d e f g h
```

## sacrifice: tagged, not labelled: ITRcp

- Lichess themes: advancedPawn advantage deflection fork long middlegame
- Our tags: deflection pin sacrifice
- Position (to move: w): `1k1r3r/1ppq4/1P4n1/3pQp2/3P2pp/2PB4/1P3PPP/R5K1 w - - 1 24`
- Solution: b6c7 d7c7 a1a8 b8a8 e5c7
- Verdict: label gap. Ra8+ Kxa8 gives a rook to deflect the king, then Qxc7.

```
8 . k . r . . . r
7 . p p q . . . .
6 . P . . . . n .
5 . . . p Q p . .
4 . . . P . . p p
3 . . P B . . . .
2 . P . . . P P P
1 R . . . . . K .
  a b c d e f g h
```

## sacrifice: tagged, not labelled: O2GfN

- Lichess themes: advantage discoveredAttack long middlegame pin
- Our tags: discoveredAttack pin sacrifice
- Position (to move: w): `1k1r4/6bp/n2r1pp1/2p2P2/3B4/3R3P/PPP2P2/2KR4 w - - 0 28`
- Solution: d4e5 f6e5 d3d6 d8d6 d1d6
- Verdict: label gap. Be5 fxe5 gives the bishop to win the pinned rook on d6.

```
8 . k . r . . . .
7 . . . . . . b p
6 n . . r . p p .
5 . . p . . P . .
4 . . . B . . . .
3 . . . R . . . P
2 P P P . . P . .
1 . . K R . . . .
  a b c d e f g h
```

## sacrifice: tagged, not labelled: tNfYR

- Lichess themes: advantage attraction fork long middlegame
- Our tags: fork sacrifice
- Position (to move: b): `6k1/6bp/p5p1/1p1pn3/3N1Bq1/P3Q3/1P1N1K2/8 b - - 7 32`
- Solution: g4f4 e3f4 e5d3 f2e3 d3f4
- Verdict: label gap. Qxf4+ Qxf4 gives the queen for a bishop, won back by the fork Nd3+.

```
8 . . . . . . k .
7 . . . . . . b p
6 p . . . . . p .
5 . p . p n . . .
4 . . . N . B q .
3 P . . . Q . . .
2 . P . N . K . .
1 . . . . . . . .
  a b c d e f g h
```

## sacrifice: labelled, not tagged: RO9uv

- Lichess themes: advantage endgame fork long sacrifice
- Our tags: hangingPiece
- Position (to move: b): `8/p3k1p1/3p4/4p3/4P3/2PP1N1r/PP2KP1n/7R b - - 1 27`
- Solution: h2f3 h1h3 f3g1 e2d2 g1h3
- Verdict: tagger miss. Nxf3 gains first, so the rook given up on h3 never leaves the settled count two pawns down.

```
8 . . . . . . . .
7 p . . . k . p .
6 . . . p . . . .
5 . . . . p . . .
4 . . . . P . . .
3 . . P P . N . r
2 P P . . K P . n
1 . . . . . . . R
  a b c d e f g h
```

## sacrifice: labelled, not tagged: yFfBn

- Lichess themes: long master masterVsMaster mate mateIn3 middlegame sacrifice smotheredMate
- Our tags: mate mateIn3 matingNet smotheredMate
- Position (to move: w): `r4q1k/1b4bp/4Q2N/p7/Pp6/3P4/1PP1p1PP/5RK1 w - - 0 29`
- Solution: f1f8 a8f8 e6g8 f8g8 h6f7
- Verdict: tagger miss, window. Rxf8+ gains at once, so the window ends before the queen sacrifice Qg8+ on ply 3.

```
8 r . . . . q . k
7 . b . . . . b p
6 . . . . Q . . N
5 p . . . . . . .
4 P p . . . . . .
3 . . . P . . . .
2 . P P . p . P P
1 . . . . . R K .
  a b c d e f g h
```

## promotion: labelled, not tagged: N0sqX

- Lichess themes: advancedPawn advantage long middlegame promotion
- Our tags: (none)
- Position (to move: w): `6k1/1Rrq1pbp/p1rP2p1/4p3/P3R3/3Q4/5PPP/6K1 w - - 0 29`
- Solution: b7c7 c6c7 d6c7 d7d3 c7c8q
- Verdict: tagger miss. The promotion c8=Q+ is on ply 5, but no swing is found, so nothing is tagged.

```
8 . . . . . . k .
7 . R r q . p b p
6 p . r P . . p .
5 . . . . p . . .
4 P . . . R . . .
3 . . . Q . . . .
2 . . . . . P P P
1 . . . . . . K .
  a b c d e f g h
```

## promotion: labelled, not tagged: UwtQ6

- Lichess themes: advancedPawn crushing deflection endgame long master pawnEndgame promotion
- Our tags: (none)
- Position (to move: b): `8/6P1/8/4p3/2p5/1k1p4/1P3P2/4K3 b - - 0 46`
- Solution: b3c2 g7g8q d3d2 e1f1 d2d1q
- Verdict: tagger miss by design. White promotes first, the material is level at the end of the line, so there is no swing and no tag.

```
8 . . . . . . . .
7 . . . . . . P .
6 . . . . . . . .
5 . . . . p . . .
4 . . p . . . . .
3 . k . p . . . .
2 . P . . . P . .
1 . . . . K . . .
  a b c d e f g h
```

## backRankMate: tagged, not labelled: ChihO

- Lichess themes: mate mateIn2 middlegame short
- Our tags: backRankMate mate mateIn2 matingNet
- Position (to move: w): `6k1/4bpp1/3p3p/3n1QPP/1p6/1P6/rqBK4/7R w - - 3 33`
- Solution: f5h7 g8f8 h7h8
- Verdict: label gap. Qh8# along the eighth rank, the king on f8 shut in by its own e7, f7 and g7.

```
8 . . . . . . k .
7 . . . . b p p .
6 . . . p . . . p
5 . . . n . Q P P
4 . p . . . . . .
3 . P . . . . . .
2 r q B K . . . .
1 . . . . . . . R
  a b c d e f g h
```

## backRankMate: tagged, not labelled: DpyV9

- Lichess themes: deflection endgame mate mateIn4 sacrifice veryLong
- Our tags: backRankMate mate mateIn4Plus matingNet sacrifice
- Position (to move: b): `2q5/p4Qpk/7p/4Np2/3Pn3/2n1P3/P4PPP/5RK1 b - - 7 29`
- Solution: c3e2 g1h1 e4f2 f1f2 c8c1 f2f1 c1f1
- Verdict: label gap. Qxf1# on the first rank, the king on h1 shut in by its own g2 and h2.

```
8 . . q . . . . .
7 p . . . . Q p k
6 . . . . . . . p
5 . . . . N p . .
4 . . . P n . . .
3 . . n . P . . .
2 P . . . . P P P
1 . . . . . R K .
  a b c d e f g h
```

## backRankMate: tagged, not labelled: o5zuI

- Lichess themes: kingsideAttack long mate mateIn3 middlegame
- Our tags: backRankMate mate mateIn3 matingNet
- Position (to move: w): `r1b2r1k/p5bp/6p1/3Qn1B1/8/6qP/PPP1B3/R2K1R2 w - - 3 22`
- Solution: f1f8 g7f8 g5f6 f8g7 d5d8
- Verdict: label gap. Qd8# along the eighth rank, the king on h8 shut in by g7 and h7.

```
8 r . b . . r . k
7 p . . . . . b p
6 . . . . . . p .
5 . . . Q n . B .
4 . . . . . . . .
3 . . . . . . q P
2 P P P . B . . .
1 R . . K . R . .
  a b c d e f g h
```

## backRankMate: tagged, not labelled: wKuJU

- Lichess themes: endgame hangingPiece mate mateIn2 short
- Our tags: backRankMate hangingPiece mate mateIn2 matingNet
- Position (to move: b): `6k1/R1q2pp1/4p2p/3p4/3Pn3/1Q6/1P3PPP/2R3K1 b - - 0 33`
- Solution: c7c1 b3d1 c1d1
- Verdict: label gap. Qxd1# on the first rank, the king on g1 shut in by f2, g2 and h2.

```
8 . . . . . . k .
7 R . q . . p p .
6 . . . . p . . p
5 . . . p . . . .
4 . . . P n . . .
3 . Q . . . . . .
2 . P . . . P P P
1 . . R . . . K .
  a b c d e f g h
```

## backRankMate: labelled, not tagged: b2Msc

- Lichess themes: backRankMate crushing endgame quietMove veryLong
- Our tags: deflection
- Position (to move: b): `2r1Rr1k/p4Qpp/8/8/2p5/2P5/Pq3PPP/4R1K1 b - - 3 24`
- Solution: c8e8 f7e8 b2f2 g1h1 h7h6 e8f8 f2f8
- Verdict: label error. No mate in the solution (it ends Qxf8+ Qxf8); the theme names the threat.

```
8 . . r . R r . k
7 p . . . . Q p p
6 . . . . . . . .
5 . . . . . . . .
4 . . p . . . . .
3 . . P . . . . .
2 P q . . . P P P
1 . . . . R . K .
  a b c d e f g h
```

## backRankMate: labelled, not tagged: i6o0S

- Lichess themes: advantage backRankMate endgame long
- Our tags: (none)
- Position (to move: w): `6k1/5ppp/p1n1p3/1p1n4/3PBP2/1R2P3/3B1PKP/r7 w - - 4 27`
- Solution: e4d5 e6d5 b3c3 c6e7 c3c7
- Verdict: label error. No mate in the solution (it ends Rc7); the theme names the threat.

```
8 . . . . . . k .
7 . . . . . p p p
6 p . n . p . . .
5 . p . n . . . .
4 . . . P B P . .
3 . R . . P . . .
2 . . . B . P K P
1 r . . . . . . .
  a b c d e f g h
```

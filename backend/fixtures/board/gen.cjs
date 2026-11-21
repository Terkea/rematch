// Generator for chessjs-reference.json (the chess.js 1.4.0 parity fixture
// used by backend/crates/chess-core/tests/board.rs, see ADR 0037).
//
//   node backend/fixtures/board/gen.cjs            # rewrite chessjs-reference.json
//   node backend/fixtures/board/gen.cjs --check    # fail if the file would change
//   node backend/fixtures/board/gen.cjs --stats    # also print coverage counts
//
// chess.js 1.4.0 is resolved from the nearest node_modules above this file
// (pnpm store or plain), or from $CHESSJS_DIR. Output is deterministic: all
// randomness comes from the seeded PRNG below.
'use strict';
const fs = require('fs');
const path = require('path');

function findUp(rel) {
  for (let d = __dirname; ; d = path.dirname(d)) {
    const p = path.join(d, rel);
    if (fs.existsSync(p)) return p;
    if (path.dirname(d) === d) return null;
  }
}
const chessDir =
  process.env.CHESSJS_DIR ||
  findUp('node_modules/.pnpm/chess.js@1.4.0/node_modules/chess.js') ||
  findUp('node_modules/chess.js');
if (!chessDir) throw new Error('chess.js not found; set CHESSJS_DIR');
const version = JSON.parse(fs.readFileSync(path.join(chessDir, 'package.json'), 'utf8')).version;
if (version !== '1.4.0') throw new Error('need chess.js 1.4.0, found ' + version);
const { Chess } = require(chessDir);
const fixturesDir = findUp('apps/worker/test/fixtures');
if (!fixturesDir) throw new Error('recorded fixtures (apps/worker/test/fixtures) not found');

const START = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1';

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function final(ch) {
  return {
    checkmate: ch.isCheckmate(),
    stalemate: ch.isStalemate(),
    insufficient_material: ch.isInsufficientMaterial(),
    threefold: ch.isThreefoldRepetition(),
  };
}

function record(name, startFen, ch, played) {
  return { name, start_fen: startFen, moves: played, final: final(ch) };
}

function step(ch, played, input) {
  const before = ch.fen();
  const r = ch.move(input);
  played.push({
    san: r.san,
    from: r.from,
    to: r.to,
    promotion: r.promotion ?? null,
    before,
    after: ch.fen(),
  });
  return r;
}

// A curated line of SAN moves.
function line(name, fen, sans) {
  const ch = new Chess(fen);
  if (ch.fen() !== fen) throw new Error(name + ': start fen is not canonical: ' + ch.fen());
  const played = [];
  for (const s of sans) step(ch, played, s);
  return record(name, fen, ch, played);
}

// Seeded random game. `weights(move)` biases the choice; stops at mate,
// stalemate, insufficient material, or (optionally) threefold.
function random(name, fen, seed, plies, weights, stopAtThreefold) {
  const rnd = mulberry32(seed);
  const ch = new Chess(fen);
  if (ch.fen() !== fen) throw new Error(name + ': start fen is not canonical: ' + ch.fen());
  const played = [];
  while (played.length < plies && !ch.isGameOver()) {
    const moves = ch.moves({ verbose: true });
    const w = moves.map((m) => weights(m));
    let x = rnd() * w.reduce((a, b) => a + b, 0);
    let i = 0;
    while (i < moves.length - 1 && x >= w[i]) x -= w[i++];
    step(ch, played, moves[i].san);
    if (stopAtThreefold && ch.isThreefoldRepetition()) break;
  }
  return record(name, fen, ch, played);
}

const capturey = (m) =>
  m.isEnPassant() ? 40 : m.isCapture() ? 8 : m.isPromotion() ? 8
    : m.isKingsideCastle() || m.isQueensideCastle() ? 20 : m.isBigPawn() ? 3 : 1;
const epHungry = (m) => (m.isEnPassant() ? 200 : m.isBigPawn() ? 6 : m.piece === 'p' ? 3 : 0.3);
const flat = () => 1;
const castly = (m) =>
  m.isKingsideCastle() || m.isQueensideCastle() ? 30 : m.piece === 'k' ? 0.2 : 1;

// Real games: SAN lists from the recorded Lichess and chess.com fixtures.
function realGames() {
  const out = [];
  const lichess = fs
    .readFileSync(path.join(fixturesDir, 'lichess-games.ndjson'), 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l));
  for (const g of lichess) {
    if (g.variant !== 'standard') continue;
    out.push(line('real_lichess_' + g.id, START, g.moves.split(' ')));
  }
  const month = JSON.parse(fs.readFileSync(path.join(fixturesDir, 'chesscom-month.json'), 'utf8'));
  for (const g of month.games) {
    if (g.rules !== 'chess') continue;
    const body = g.pgn.split('\n\n').slice(1).join('\n\n');
    const sans = body
      .replace(/\{[^}]*\}/g, ' ')
      .replace(/\d+\.(\.\.)?/g, ' ')
      .split(/\s+/)
      .filter((t) => t && !/^(1-0|0-1|1\/2-1\/2|\*)$/.test(t));
    out.push(line('real_chesscom_' + g.uuid, START, sans));
  }
  return out;
}

function curated() {
  const c = [];
  const K2 = 'r3k2r/3p1p2/8/8/8/8/3P1P2/R3K2R w KQkq - 0 1';
  const sp = (s) => s.split(' ');
  c.push(line('a_scholars_mate', START, sp('e4 e5 Qh5 Nc6 Bc4 Nf6 Qxf7#')));
  c.push(line('b_promotion_capture', START, sp('e4 d5 exd5 c6 dxc6 Nf6 cxb7 Nbd7 bxa8=Q Nb6')));
  c.push(line('c_both_castlings', START, sp('e4 e5 Nf3 Nc6 Bc4 Bc5 O-O Nf6 d3 d6 Bg5 Qe7 Nc3 Bd7 Qd2 O-O-O')));
  c.push(line('d_en_passant', START, sp('e4 a6 e5 d5 exd6')));
  c.push(line('e_double_push_no_neighbour', START, ['e4']));
  c.push(line('f_ep_pinned_not_printed', 'k7/3p4/8/K3P2r/8/8/8/8 b - - 0 1', ['d5']));
  c.push(line('g_knight_file_disambiguation', '4k3/8/8/8/8/8/8/1N2KN2 w - - 0 1', sp('Nbd2 Kd8 Ne3')));
  c.push(line('g_knight_rank_disambiguation', '4k3/8/8/1N6/8/1N6/8/4K3 w - - 0 1', sp('N3d4 Kd8 Nc3')));
  c.push(line('g_queen_rank_disambiguation', '1k6/8/8/8/7Q/8/8/K3Q2Q w - - 0 1', ['Q4e4']));
  c.push(line('g_queen_square_disambiguation', '1k6/8/8/8/7Q/8/8/K3Q2Q w - - 0 1', ['Qh1e4']));
  c.push(line('g_queen_file_disambiguation', '1k6/8/8/8/7Q/8/8/K3Q2Q w - - 0 1', ['Qee4']));
  c.push(line('h_sam_loyd_stalemate', START, sp('e3 a5 Qh5 Ra6 Qxa5 h5 h4 Rah6 Qxc7 f6 Qxd7+ Kf7 Qxb7 Qd3 Qxb8 Qh7 Qxc8 Kg6 Qe6')));
  c.push(line('i_threefold', START, sp('Nf3 Nf6 Ng1 Ng8 Nf3 Nf6 Ng1 Ng8')));
  c.push(line('j_underpromotion_and_check', '7k/1P6/8/8/8/8/8/K7 w - - 0 1', ['b8=N']));
  c.push(line('k_insufficient_opposite_bishops', '8/8/8/8/1b6/8/B1k5/K7 b - - 0 1', []));
  c.push(line('k_insufficient_kb_vs_k', '8/8/8/8/8/2k5/8/K1B5 w - - 0 1', []));

  // Double (square) disambiguation: three queens, three knights, three rooks.
  c.push(line('m_queen_square_three_queens', '8/7k/8/Q3Q3/8/8/8/4Q2K w - - 0 1', ['Qe5c3']));
  c.push(line('m_knight_square_three_knights', '7k/8/8/8/8/1N3N2/8/1N5K w - - 0 1', ['Nb3d2']));
  c.push(line('m_rook_square_three_rooks', '7k/8/8/8/R3R3/8/8/R6K w - - 0 1', ['Ra4a2']));

  // Castling, all four combinations, from a position with all rights.
  c.push(line('n_castle_wk_bk', K2, ['O-O', 'O-O']));
  c.push(line('n_castle_wq_bq', K2, ['O-O-O', 'O-O-O']));
  c.push(line('n_castle_wk_bq', K2, ['O-O', 'O-O-O']));
  c.push(line('n_castle_wq_bk', K2, ['O-O-O', 'O-O']));
  c.push(line('n_castle_with_check', '3k4/8/8/8/8/8/8/R3K3 w Q - 0 1', ['O-O-O+']));

  // Printed ep squares and ep captures by hand (both colours).
  c.push(line('o_ep_white_and_black', START, sp('e4 a6 e5 d5 exd6 Nf6 d4 e6 d5 c5 dxc6')));
  c.push(line('o_ep_black_captures', START, sp('a3 h5 a4 h4 g4 hxg3 h3 g2')));
  c.push(line('o_ep_gone_after_one_move', START, sp('e4 a6 e5 d5 Nf3 Nc6 Nc3 Nf6')));

  // Mates and stalemates.
  c.push(line('p_fools_mate', START, sp('f3 e5 g4 Qh4#')));
  c.push(line('p_back_rank_mate', '6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1', ['Ra8#']));
  c.push(line('p_smothered_mate', '6rk/6pp/8/6N1/8/8/8/K7 w - - 0 1', ['Nf7#']));
  c.push(line('p_queen_stalemate', 'k7/8/1K6/8/8/8/8/2Q5 w - - 0 1', ['Qc7']));
  c.push(line('p_pawn_stalemate', '7k/7P/7K/8/8/8/8/8 b - - 0 1', []));

  // Threefold by repetition with a rook shuffle, and a two-fold near miss.
  c.push(line('q_threefold_rooks', 'r3k3/8/8/8/8/8/8/R3K3 w - - 0 1', sp('Ra2 Ra7 Ra1 Ra8 Ra2 Ra7 Ra1 Ra8')));
  c.push(line('q_two_fold_only', 'r3k3/8/8/8/8/8/8/R3K3 w - - 0 1', sp('Ra2 Ra7 Ra1 Ra8 Ra2 Ra7 Ra1')));

  // Halfmove and fullmove counters above engine caps.
  c.push(line('r_high_clocks', '4k3/8/8/8/8/8/8/R3K3 w - - 140 90', sp('Ra2 Kd7 Ra3 Ke7 Ra4 Kd7')));
  c.push(line('r_clock_reset_by_capture', '4k3/8/8/p7/8/8/8/R3K3 w - - 120 70', sp('Ra2 Kd7 Rxa5 Kc6')));
  return c;
}

function seeded() {
  const c = [];
  const PAWNS = '4k3/pppppppp/8/8/8/8/PPPPPPPP/4K3 w - - 0 1';
  const CASTLE = 'r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1';
  const OPEN = 'r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1';
  for (let i = 0; i < 12; i++) c.push(random('random_' + i, START, 1000 + i, 100, capturey));
  for (let i = 0; i < 8; i++) c.push(random('random_ep_' + i, PAWNS, 2000 + i, 80, epHungry));
  for (let i = 0; i < 4; i++) c.push(random('random_castle_' + i, CASTLE, 3000 + i, 60, castly));
  for (let i = 0; i < 4; i++) c.push(random('random_open_' + i, OPEN, 3500 + i, 80, capturey));
  // Endgame hunts: keep the first seeds that end in mate, stalemate or threefold.
  const endgames = [
    '7k/8/8/8/8/8/8/KQ6 w - - 0 1',
    '6k1/8/8/8/8/8/8/KR5R w - - 0 1',
    '4k3/8/8/8/8/8/8/R3K2R w - - 0 1',
    '8/8/8/3k4/8/3K4/2Q5/8 w - - 0 1',
  ];
  const want = { checkmate: 6, stalemate: 4, threefold: 4 };
  const have = { checkmate: 0, stalemate: 0, threefold: 0 };
  for (let s = 0; s < 4000; s++) {
    if (Object.keys(want).every((k) => have[k] >= want[k])) break;
    const g = random('hunt', endgames[s % endgames.length], 5000 + s, 150, flat, true);
    const kind = g.final.checkmate ? 'checkmate' : g.final.stalemate ? 'stalemate' : g.final.threefold ? 'threefold' : null;
    if (!kind || have[kind] >= want[kind]) continue;
    have[kind]++;
    g.name = 'hunt_' + kind + '_' + have[kind];
    c.push(g);
  }
  return c;
}

function coverage(cases) {
  const s = {
    cases: cases.length, moves: 0, ep_captures: 0, printed_ep_squares: 0,
    file_disamb: 0, rank_disamb: 0, square_disamb: 0,
    wk_castles: 0, wq_castles: 0, bk_castles: 0, bq_castles: 0,
    promotions: 0, underpromotions: 0, checks_or_mates: 0,
    checkmate_games: 0, stalemate_games: 0, threefold_games: 0, insufficient_games: 0,
    fen_clock_above_100: 0,
  };
  for (const c of cases) {
    if (c.final.checkmate) s.checkmate_games++;
    if (c.final.stalemate) s.stalemate_games++;
    if (c.final.threefold) s.threefold_games++;
    if (c.final.insufficient_material) s.insufficient_games++;
    if (+c.start_fen.split(' ')[4] > 100) s.fen_clock_above_100++;
    for (const m of c.moves) {
      s.moves++;
      const white = m.before.split(' ')[1] === 'w';
      const r = new Chess(m.before).move(m.san);
      if (r.isEnPassant()) s.ep_captures++;
      if (m.after.split(' ')[3] !== '-') s.printed_ep_squares++;
      if (r.promotion) {
        s.promotions++;
        if (r.promotion !== 'q') s.underpromotions++;
      }
      const mm = r.san.match(/^[NBRQK]([a-h]?[1-8]?)x?[a-h][1-8]/);
      if (mm && mm[1].length === 2) s.square_disamb++;
      else if (mm && /[a-h]/.test(mm[1])) s.file_disamb++;
      else if (mm && mm[1]) s.rank_disamb++;
      if (r.san.startsWith('O-O-O')) s[white ? 'wq_castles' : 'bq_castles']++;
      else if (r.san.startsWith('O-O')) s[white ? 'wk_castles' : 'bk_castles']++;
      if (/[+#]$/.test(r.san)) s.checks_or_mates++;
    }
  }
  return s;
}

function main() {
  const cases = [...curated(), ...realGames(), ...seeded()];
  if (new Set(cases.map((c) => c.name)).size !== cases.length) throw new Error('duplicate case names');
  const text = JSON.stringify({ chessjs_version: version, cases }, null, 1);
  const out = path.join(__dirname, 'chessjs-reference.json');
  if (process.argv.includes('--stats')) console.log(coverage(cases));
  if (process.argv.includes('--check')) {
    if (fs.readFileSync(out, 'utf8') !== text) {
      console.error('chessjs-reference.json is stale');
      process.exit(1);
    }
    console.log('chessjs-reference.json is up to date');
  } else {
    fs.writeFileSync(out, text);
  }
}
main();

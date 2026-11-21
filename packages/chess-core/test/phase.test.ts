import { describe, expect, it } from "vitest";
import { detectPhases } from "../src/phase.js";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const ROOK_ENDING = "8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1";
const QR_EACH = "3qr1k1/pppp1ppp/8/8/8/8/PPPP1PPP/3QR1K1 w - - 0 1";
// Queenless, 16 per side (R+R+B+N): 32 total, over both limits.
const QUEENLESS_32 = "1rbn1rk1/pppppppp/8/8/8/8/PPPPPPPP/1RBN1RK1 w - - 0 1";
// Queenless, white 16 (R+R+B+N) and black 10 (R+R): 26 total, only the total rule applies.
const QUEENLESS_26 = "1r3rk1/pppppppp/8/8/8/8/PPPPPPPP/1RBN1RK1 w - - 0 1";

const repeat = (fen: string, n: number) => Array.from({ length: n }, () => fen);

describe("detectPhases", () => {
  it("is opening through ply 20 and middlegame from 21 by default", () => {
    const phases = detectPhases(repeat(START, 25));
    expect(phases.slice(0, 20).every((p) => p === "opening")).toBe(true);
    expect(phases.slice(20).every((p) => p === "middlegame")).toBe(true);
  });

  it("detects an endgame once the opening is over", () => {
    const phases = detectPhases(repeat(ROOK_ENDING, 22));
    expect(phases[19]).toBe("opening");
    expect(phases[20]).toBe("endgame");
    expect(phases[21]).toBe("endgame");
  });

  it("treats Q+R each (28 total, 14 per side) as middlegame", () => {
    expect(detectPhases(repeat(QR_EACH, 22))[21]).toBe("middlegame");
  });

  it("applies the queenless total material rule (<= 26)", () => {
    expect(detectPhases(repeat(QUEENLESS_32, 22))[21]).toBe("middlegame");
    expect(detectPhases(repeat(QUEENLESS_26, 22))[21]).toBe("endgame");
  });

  it("never goes backwards from endgame", () => {
    const phases = detectPhases([...repeat(ROOK_ENDING, 21), QR_EACH, START]);
    expect(phases[20]).toBe("endgame");
    expect(phases[21]).toBe("endgame");
    expect(phases[22]).toBe("endgame");
  });

  it("extends the opening to bookExitPly", () => {
    const phases = detectPhases(repeat(START, 30), 26);
    expect(phases[25]).toBe("opening");
    expect(phases[26]).toBe("middlegame");
  });

  it("caps the opening at ply 30", () => {
    const phases = detectPhases(repeat(START, 33), 40);
    expect(phases[29]).toBe("opening");
    expect(phases[30]).toBe("middlegame");
  });
});

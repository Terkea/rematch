"use client";

import { games } from "@ci/ui/fixtures";
import { Icon, MagnifyingGlassIcon } from "@ci/ui/icons";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@ci/ui/primitives";
import { Scoresheet, type ScoresheetMove } from "@ci/ui/review";
import { useState } from "react";

const game = games.middlegameBlunder;
/** The first 16 plies of a real fixture game, enough rows to stack. */
const moves: ScoresheetMove[] = game.plies.slice(0, 16).map((p) => ({
  ply: p.ply,
  color: p.color,
  san: p.san,
  judgement: p.judgement,
  best: p.bestUci !== null && p.uci === p.bestUci,
  mine: p.color === game.playerColor,
}));

function ScoresheetFixture() {
  const [ply, setPly] = useState(8);
  return (
    <section data-fixture="scoresheet">
      <Scoresheet
        moves={moves}
        currentPly={ply}
        onSelectPly={setPly}
        label="Moves, touch fixture"
        className="h-96"
      />
    </section>
  );
}

/**
 * Fixture for apps/web/e2e/touch-targets.spec.ts: each control the primitives give a 44px coarse-
 * pointer hit area, laid out with the stacking gaps DESIGN.md asks for (12px between stacked sm
 * controls, 4px between md ones), so the hit test also proves neighbouring areas don't overlap.
 * Guarded by the lab layout, so production serves a 404 unless LAB_ENABLED=1.
 */
export default function TouchTargetsFixture() {
  return (
    <main className="mx-auto flex max-w-3xl flex-col gap-8 px-(--space-gutter) py-8">
      <h1 className="font-serif text-3xl font-semibold">Touch targets</h1>

      <section className="flex flex-col gap-1" data-fixture="md-stack">
        <Button data-target="md">Review game</Button>
        <Button data-target="md" variant="outline">
          <Icon icon={MagnifyingGlassIcon} />
          Find a game
        </Button>
        <Button data-target="md" variant="danger">
          Delete account
        </Button>
      </section>

      <section className="flex flex-col items-start gap-3" data-fixture="sm-stack">
        <Button data-target="sm" size="sm" variant="outline">
          Step back
        </Button>
        <Button data-target="sm" size="sm" variant="quiet">
          Skip
        </Button>
        <Button data-target="sm" size="sm">
          Start drill
        </Button>
      </section>

      <section className="rounded-sheet bg-sheet p-(--space-panel-pad)" data-fixture="tabs">
        <Tabs defaultValue="moves">
          <TabsList>
            <TabsTrigger data-target="tab" value="moves">
              Moves
            </TabsTrigger>
            <TabsTrigger data-target="tab" value="mistakes">
              Mistakes
            </TabsTrigger>
            <TabsTrigger data-target="tab" value="plans">
              Plans
            </TabsTrigger>
          </TabsList>
          <TabsContent value="moves">9…b5? lets 10. Nxb5 open the c-file with tempo.</TabsContent>
          <TabsContent value="mistakes">Five marked moments.</TabsContent>
          <TabsContent value="plans">Develop, then castle.</TabsContent>
        </Tabs>
      </section>

      <ScoresheetFixture />

      <Dialog>
        <DialogTrigger render={<Button variant="outline" />}>Disconnect</DialogTrigger>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Disconnect chess.com?</DialogTitle>
            <DialogDescription>
              Your reviewed games stay. New games stop importing until you connect again.
            </DialogDescription>
          </DialogHeader>
        </DialogContent>
      </Dialog>
    </main>
  );
}

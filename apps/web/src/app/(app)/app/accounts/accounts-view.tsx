"use client";

import { CheckCircleIcon, ClockIcon, Icon, WarningIcon } from "@ci/ui/icons";
import { AnimatePresence, dur, ease, motion, spring, useReducedMotion } from "@ci/ui/motion";
import { toast } from "@ci/ui/overlays";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@ci/ui/primitives";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { authClient } from "../../../../lib/auth-client";
import type { LinkedAccountView } from "../../../../server/accounts/list";
import { PendingButton } from "../../../ui/pending-button";
import { PageHeading } from "../../_shell/page-heading";
import {
  accountsKey,
  fetchAccounts,
  linkErrorMessage,
  removeAccountLink,
  type StartedLink,
  startChesscomLink,
  verifyChesscomLink,
} from "./api";
import { ChesscomLinkForm } from "./chesscom-link-form";

export interface AccountsViewProps {
  initialAccounts: LinkedAccountView[];
  /** `CHESSCOM_ENABLED`: the chess.com form and rows. */
  chesscom: boolean;
  /** Lichess linking is configured (deferred by ADR 0040 until Task 18). */
  lichess: boolean;
}

const EMPTY = "Connect a Lichess or chess.com account to import your games.";
const NOT_YET = "Not there yet. Try again in a minute.";
const READ_ONLY = "We read your public games. Nothing is ever posted to your account.";
/** What chess.com verification asks of the player, as a margin note on the empty page. */
const STEPS = [
  { title: "Your chess.com username", detail: "We check the account exists." },
  { title: "Put the code in Location", detail: "On your chess.com profile. Remove it afterwards." },
  { title: "Check again", detail: "The account is yours once chess.com shows the code." },
];
/** chess.com answers one profile request per username a minute (ADR-WEB-5). */
const FIRST_CHECK_MS = 60_000;

const platformName = { chesscom: "chess.com", lichess: "Lichess" } as const;

/** Re-renders every second while `active`, for the check-again countdown. */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return now;
}

function StatusLine({ account, fresh }: { account: LinkedAccountView; fresh: boolean }) {
  const reduced = useReducedMotion();
  if (account.status === "ok") {
    return (
      <motion.span
        key="ok"
        className="inline-flex items-center gap-1.5 text-sm font-medium text-good-text"
        // A just-verified badge lands like a glyph badge (`land`); otherwise it is simply there.
        initial={fresh && !reduced ? { scale: 0.6, opacity: 0 } : false}
        animate={{ scale: 1, opacity: 1 }}
        transition={spring.land}
      >
        <Icon icon={CheckCircleIcon} size={16} weight="fill" />
        {account.platform === "lichess" ? "Connected" : "Verified"}
      </motion.span>
    );
  }
  const text =
    account.status === "pending_verification"
      ? "Waiting for the code"
      : account.status === "expiring"
        ? "Access ends soon"
        : account.platform === "lichess"
          ? "Access ended"
          : "Code expired";
  return (
    <span className="inline-flex items-center gap-1.5 text-sm text-ink-soft">
      <Icon icon={account.status === "pending_verification" ? ClockIcon : WarningIcon} size={16} />
      {text}
    </span>
  );
}

interface RowProps {
  account: LinkedAccountView;
  now: number;
  nextCheckAt: number | undefined;
  result: string | undefined;
  checking: boolean;
  restarting: boolean;
  fresh: boolean;
  onCheck: () => void;
  onRestart: () => void;
  onRemove: () => void;
  onReconnect: () => void;
}

function AccountRow(props: RowProps) {
  const { account, now, nextCheckAt } = props;
  const reduced = useReducedMotion();
  const wait = nextCheckAt ? Math.ceil((nextCheckAt - now) / 1000) : 0;
  const pending = account.status === "pending_verification" && account.code;

  async function copy(code: string) {
    try {
      await navigator.clipboard.writeText(code);
      toast("Code copied");
    } catch {
      toast.error("Copying isn't allowed here. Select the code and copy it.");
    }
  }

  return (
    <motion.li
      layout={reduced ? false : "position"}
      initial={reduced ? false : { opacity: 0, scale: 0.98 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.98, transition: { duration: dur.quick, ease: [...ease.out] } }}
      transition={{ ...spring.page, opacity: { duration: dur.quick } }}
      className="flex flex-col gap-4 py-5 first:pt-0 last:pb-0"
    >
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <div className="flex min-w-0 flex-col">
          <span className="text-sm text-ink-faint">{platformName[account.platform]}</span>
          <span className="truncate font-serif text-xl font-semibold text-ink">
            {account.displayName}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <StatusLine account={account} fresh={props.fresh} />
          <Button
            variant="quiet"
            size="sm"
            aria-label={`Remove ${account.displayName}`}
            onClick={props.onRemove}
          >
            Remove
          </Button>
        </div>
      </div>

      {pending ? (
        <div className="flex flex-col gap-4 rounded-field bg-well p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <span className="font-serif text-2xl font-semibold tracking-glyph tabular-nums lining-nums text-ink select-all">
              {account.code}
            </span>
            <Button variant="outline" size="sm" onClick={() => copy(account.code as string)}>
              Copy code
            </Button>
          </div>
          <p className="max-w-prose text-sm text-ink-soft">
            {`Add ${account.code} to the Location field on your chess.com profile, then check again. chess.com can take up to 12 hours to show profile changes.`}
          </p>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <PendingButton
              size="sm"
              pending={props.checking}
              pendingLabel="Checking"
              unavailable={wait > 0}
              onClick={props.onCheck}
            >
              Check again
            </PendingButton>
            <div aria-live="polite" className="flex flex-col text-sm">
              {props.result ? <span className="text-ink">{props.result}</span> : null}
              {wait > 0 ? (
                <span className="text-ink-faint tabular-nums">You can check again in {wait} s</span>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}

      {account.platform === "chesscom" && account.status === "expired" ? (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-field bg-well p-4">
          <p className="text-sm text-ink-soft">
            This code has expired. Get a new one to try again.
          </p>
          <PendingButton
            size="sm"
            variant="outline"
            pending={props.restarting}
            pendingLabel="Getting a code"
            onClick={props.onRestart}
          >
            Get a new code
          </PendingButton>
        </div>
      ) : null}

      {account.platform === "lichess" &&
      (account.status === "expiring" || account.status === "expired") ? (
        <div>
          <Button size="sm" variant="outline" onClick={props.onReconnect}>
            Reconnect Lichess
          </Button>
        </div>
      ) : null}
    </motion.li>
  );
}

function connectLichess() {
  void authClient.linkSocial({
    provider: "lichess",
    callbackURL: "/app/accounts?connected=lichess",
    errorCallbackURL: "/app/accounts?error=lichess_denied",
  });
}

/**
 * The linked accounts and how to add one. The server renders the first list; TanStack Query keeps
 * it fresh after every mutation. Each action shows its own pending state, never a spinner (B18).
 */
export function AccountsView({ initialAccounts, chesscom, lichess }: AccountsViewProps) {
  const queryClient = useQueryClient();
  const accounts = useQuery({
    queryKey: accountsKey,
    queryFn: fetchAccounts,
    initialData: initialAccounts,
    staleTime: 30_000,
  }).data;

  const [nextCheck, setNextCheck] = useState<Record<string, number>>({});
  const [results, setResults] = useState<Record<string, string>>({});
  const [fresh, setFresh] = useState<Set<string>>(() => new Set());
  const [removing, setRemoving] = useState<LinkedAccountView | null>(null);
  const now = useNow(Object.values(nextCheck).some((at) => at > Date.now()));

  const refresh = () => queryClient.invalidateQueries({ queryKey: accountsKey });

  // Back from the Lichess consent screen (Task 18).
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("connected") === "lichess") toast.success("Lichess connected");
    else if (params.get("error") === "lichess_denied")
      toast.error("Lichess wasn't connected. You can try again.");
    else return;
    window.history.replaceState(null, "", window.location.pathname);
  }, []);

  function linked(link: StartedLink) {
    toast.success("chess.com connected");
    if (link.code) setNextCheck((m) => ({ ...m, [link.linkId]: Date.now() + FIRST_CHECK_MS }));
    setResults(({ [link.linkId]: _, ...rest }) => rest);
    void refresh();
  }

  const verify = useMutation({
    mutationFn: verifyChesscomLink,
    onSuccess: (result, linkId) => {
      if (result.status === "verified") {
        toast.success("chess.com account verified");
        setFresh((s) => new Set(s).add(linkId));
        setResults(({ [linkId]: _, ...rest }) => rest);
      } else if (result.status === "pending") {
        setResults((m) => ({ ...m, [linkId]: NOT_YET }));
        setNextCheck((m) => ({ ...m, [linkId]: Date.parse(result.nextCheckAt) }));
      }
      void refresh();
    },
    onError: (error, linkId) => setResults((m) => ({ ...m, [linkId]: linkErrorMessage(error) })),
  });

  const restart = useMutation({
    mutationFn: (account: LinkedAccountView) => startChesscomLink(account.username),
    onSuccess: linked,
    onError: (error, account) => toast.error(linkErrorMessage(error, account.username)),
  });

  const remove = useMutation({
    mutationFn: (account: LinkedAccountView) => removeAccountLink(account.linkId),
    onSuccess: (_, account) => {
      queryClient.setQueryData<LinkedAccountView[]>(accountsKey, (list) =>
        list?.filter((a) => a.linkId !== account.linkId),
      );
      toast.success(`${account.displayName} removed`);
      setRemoving(null);
      void refresh();
    },
    onError: (error) => toast.error(linkErrorMessage(error)),
  });

  const visible = accounts.filter((a) => chesscom || a.platform !== "chesscom");
  const connect = (
    <>
      {chesscom ? (
        <ChesscomLinkForm
          onLinked={linked}
          primary={!visible.some((a) => a.status === "pending_verification")}
        />
      ) : null}
      {lichess ? (
        <Button variant="outline" className="w-full" onClick={connectLichess}>
          Connect Lichess
        </Button>
      ) : null}
    </>
  );

  return (
    <div className="flex flex-col gap-8">
      <PageHeading title="Accounts" lede="The accounts whose games you want reviewed." />

      {visible.length === 0 ? (
        // Nothing linked yet: one sheet that leads with the way in, and a margin note on what the
        // verification asks of the player (review of Task 21).
        <section
          aria-labelledby="connect-title"
          data-surface="sheet"
          className="grid gap-8 rounded-sheet bg-sheet p-(--space-panel-pad) lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] lg:gap-12"
        >
          <div className="flex flex-col gap-5">
            <div className="flex flex-col gap-2">
              <h2 id="connect-title" className="font-serif text-2xl font-semibold text-ink">
                Connect an account
              </h2>
              <p className="text-base text-ink-soft">{EMPTY}</p>
              <p className="text-sm text-ink-faint">{READ_ONLY}</p>
            </div>
            {connect}
          </div>
          {chesscom ? (
            <ol
              aria-label="How linking works"
              className="flex flex-col gap-5 self-start rounded-field bg-well p-(--space-panel-pad)"
            >
              {STEPS.map((step, i) => (
                <li key={step.title} className="flex gap-4">
                  <span className="w-6 shrink-0 font-serif text-2xl font-semibold tabular-nums lining-nums text-ink-faint">
                    {i + 1}
                  </span>
                  <div className="flex flex-col gap-0.5 pt-1">
                    <p className="text-base font-medium text-ink">{step.title}</p>
                    <p className="text-sm text-ink-soft">{step.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          ) : null}
        </section>
      ) : (
        <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
          <section
            aria-labelledby="linked-title"
            data-surface="sheet"
            className="flex flex-col gap-5 rounded-sheet bg-sheet p-(--space-panel-pad)"
          >
            <h2 id="linked-title" className="font-serif text-xl font-semibold text-ink">
              Linked accounts
            </h2>
            <ul className="flex flex-col divide-y divide-rule">
              <AnimatePresence initial={false} mode="popLayout">
                {visible.map((account) => (
                  <AccountRow
                    key={account.linkId}
                    account={account}
                    now={now}
                    nextCheckAt={nextCheck[account.linkId]}
                    result={results[account.linkId]}
                    checking={verify.isPending && verify.variables === account.linkId}
                    restarting={restart.isPending && restart.variables?.linkId === account.linkId}
                    fresh={fresh.has(account.linkId)}
                    onCheck={() => verify.mutate(account.linkId)}
                    onRestart={() => restart.mutate(account)}
                    onRemove={() => setRemoving(account)}
                    onReconnect={connectLichess}
                  />
                ))}
              </AnimatePresence>
            </ul>
          </section>

          {chesscom || lichess ? (
            <section
              aria-labelledby="connect-title"
              data-surface="sheet"
              className="flex flex-col gap-5 rounded-sheet bg-sheet p-(--space-panel-pad)"
            >
              <div className="flex flex-col gap-1">
                <h2 id="connect-title" className="font-serif text-xl font-semibold text-ink">
                  Connect an account
                </h2>
                <p className="text-sm text-ink-soft">{READ_ONLY}</p>
              </div>
              {connect}
            </section>
          ) : null}
        </div>
      )}

      <Dialog open={removing !== null} onOpenChange={(open) => (open ? null : setRemoving(null))}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove {removing?.displayName}?</DialogTitle>
            <DialogDescription>
              Your imported games stay, but they won't sync to your account.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
            <PendingButton
              variant="danger"
              pending={remove.isPending}
              pendingLabel="Removing"
              onClick={() => removing && remove.mutate(removing)}
            >
              Remove account
            </PendingButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

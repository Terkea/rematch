"use client";

import { CaretDownIcon, Icon, WarningCircleIcon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { inputClasses } from "@ci/ui/primitives";
import { type FormEvent, useId, useMemo, useState } from "react";
import { PendingButton } from "../../../ui/pending-button";

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const HINT = "We ask so we can keep younger players' accounts private by default.";
const CHOOSE = "Choose the month and year you were born.";
const FUTURE = "That month hasn't happened yet. Choose your birth month.";
const FAILED = "Your answer couldn't be saved. Try again.";

export interface AgeFormProps {
  today?: Date;
  navigate?: (href: string) => void;
}

function Select({
  label,
  value,
  onChange,
  options,
  describedBy,
  invalid,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
  describedBy: string;
  invalid: boolean;
}) {
  const id = useId();
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
      <label htmlFor={id} className="w-fit text-sm font-medium text-ink">
        {label}
      </label>
      <div className="relative">
        <select
          id={id}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-describedby={describedBy}
          aria-invalid={invalid || undefined}
          className={cn(inputClasses, "appearance-none pr-10")}
        >
          <option value="" disabled>
            {label}
          </option>
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <Icon
          icon={CaretDownIcon}
          size={16}
          className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-ink-soft"
        />
      </div>
    </div>
  );
}

/**
 * Month and year only, never a full date (ADR-WEB-6). The answer is recorded once; the server
 * decides where it leads (`/app/accounts`, or `/blocked` for an under-13).
 */
export function AgeForm({
  today = new Date(),
  navigate = (href) => window.location.assign(href),
}: AgeFormProps) {
  const base = useId();
  const [month, setMonth] = useState("");
  const [year, setYear] = useState("");
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState(false);

  const years = useMemo(() => {
    const list: { value: string; label: string }[] = [];
    for (let y = today.getFullYear(); y >= 1900; y--)
      list.push({ value: String(y), label: String(y) });
    return list;
  }, [today]);
  const months = MONTHS.map((name, i) => ({ value: String(i + 1), label: name }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (pending) return;
    if (!month || !year) {
      setError(CHOOSE);
      return;
    }
    setError(undefined);
    setPending(true);
    try {
      const res = await fetch("/api/age", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ year: Number(year), month: Number(month) }),
      });
      if (res.ok) {
        const { redirectTo } = (await res.json()) as { redirectTo: string };
        navigate(redirectTo);
        return;
      }
      // Already answered (another tab): carry on to the app.
      if (res.status === 409) return navigate("/app/accounts");
      if (res.status === 401) return navigate("/sign-in");
      setError(res.status === 400 ? FUTURE : FAILED);
    } catch {
      setError(FAILED);
    }
    setPending(false);
  }

  const hintId = `${base}-hint`;
  const errorId = `${base}-error`;
  const describedBy = error ? `${hintId} ${errorId}` : hintId;

  return (
    <form method="post" onSubmit={submit} noValidate className="flex flex-col gap-8">
      <fieldset className="flex flex-col gap-5">
        <legend className="mb-3">
          <h1 id="auth-title" className="font-serif text-3xl font-semibold text-ink">
            When were you born?
          </h1>
        </legend>
        <p id={hintId} className="text-lg text-ink-soft">
          {HINT}
        </p>
        <div className="flex gap-3">
          <Select
            label="Month"
            value={month}
            onChange={setMonth}
            options={months}
            describedBy={describedBy}
            invalid={Boolean(error) && !month}
          />
          <Select
            label="Year"
            value={year}
            onChange={setYear}
            options={years}
            describedBy={describedBy}
            invalid={Boolean(error) && !year}
          />
        </div>
        <div aria-live="polite" className="empty:hidden">
          {error ? (
            <p id={errorId} className="flex items-start gap-1.5 text-sm text-pen-text">
              <Icon icon={WarningCircleIcon} size={16} className="mt-0.5" />
              {error}
            </p>
          ) : null}
        </div>
      </fieldset>
      <PendingButton type="submit" className="w-full" pending={pending} pendingLabel="Saving">
        Continue
      </PendingButton>
    </form>
  );
}

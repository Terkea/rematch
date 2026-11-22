"use client";

import { Field, Input } from "@ci/ui/primitives";

/**
 * A native date input in the app's `Field`, for the filter bar's GET forms. A client module of
 * its own: `Field` reads its one child with `Children.only`, which a server-rendered `Input`
 * handed across the server/client boundary does not satisfy.
 */
export function DateField({
  label,
  name,
  value,
}: {
  label: string;
  name: string;
  value: string | null;
}) {
  return (
    <Field label={label} className="w-40">
      <Input type="date" name={name} defaultValue={value ?? ""} min="2000-01-01" max="2100-12-31" />
    </Field>
  );
}

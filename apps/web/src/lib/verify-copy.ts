// The calm states an unverified link shows in place of the coach's work (holistic F10, the
// owner's ruling "analysis only"), with the one action that lifts them. No server import, so
// client components can use it.

export const VERIFY_HREF = "/app/accounts";
export const VERIFY_ACTION = { label: "Verify this account", href: VERIFY_HREF } as const;

export const VERIFY_REPORT_TITLE = "Verify this account to see your coach's report";
export const VERIFY_NOTE_TITLE = "Verify this account to see your coach's note";
export const VERIFY_COACH_TITLE = "Verify to ask your coach";
export const VERIFY_BODY =
  "You can see the analysis of this account's public games. Its coaching is for the account's owner: verify that it is yours to read it.";

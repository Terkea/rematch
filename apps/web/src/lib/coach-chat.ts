/**
 * Whether the coach chat pages (`/app/coach`, M5b Task 9) are live. Until they ship, nothing
 * links to them: the header hides Coach, the command menu drops "Ask your coach", and the report,
 * weakness and area pages leave out their "Ask your coach" links (M5b T11, coordinator ruling:
 * the owner tests the rest first). Task 9's merge turns this on.
 */
export function coachChatLive(): boolean {
  return false;
}

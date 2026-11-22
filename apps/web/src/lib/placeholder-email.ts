/**
 * Accounts created by a Lichess sign-up have this non-routable address (RFC 2606 `.invalid`)
 * instead of any email from Lichess (ADR-WEB-4). It is never mailed and never shown as an email.
 * Shared by the server (sign-up, mail, unlinking) and the client (the signed-in identity).
 */
export const PLACEHOLDER_EMAIL_DOMAIN = "lichess.chess-improver.invalid";

export function placeholderEmail(lichessId: string): string {
  return `${lichessId.toLowerCase()}@${PLACEHOLDER_EMAIL_DOMAIN}`;
}

export function isPlaceholderEmail(email: string): boolean {
  return email.trim().toLowerCase().endsWith(`@${PLACEHOLDER_EMAIL_DOMAIN}`);
}

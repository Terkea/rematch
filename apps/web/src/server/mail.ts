import nodemailer, { type Transporter } from "nodemailer";
import { brand } from "../brand";
import { getEnv } from "../env";
import { isPlaceholderEmail } from "../lib/placeholder-email";

export type MailMessage = { to: string; subject: string; text: string; html: string };

let transport: Transporter | undefined;

/**
 * Sends one message over `SMTP_URL` (Mailpit in every environment so far, §18.5). This is the one
 * mail path: a Lichess placeholder address (ADR-WEB-4) is never sent anything, whatever the caller.
 */
export async function sendMail(msg: MailMessage): Promise<void> {
  if (isPlaceholderEmail(msg.to)) return;
  const env = getEnv();
  transport ??= nodemailer.createTransport(env.SMTP_URL);
  await transport.sendMail({ from: env.MAIL_FROM, ...msg });
}

const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
  );

/**
 * The magic-link email (Task 17 copy). It opens with the lockup, hosted by the app itself
 * (`public/brand`), so the image comes from the same origin as the link; its alt text names it
 * when a mail client blocks images.
 */
export function magicLinkEmail(to: string, url: string): MailMessage {
  const line = `Use this link to sign in to ${brand.name}. It works once and expires in 10 minutes.`;
  const logo = new URL("/brand/rematch-lockup-mail.png", url).toString();
  return {
    to,
    subject: "Your sign-in link",
    text: `${line}\n\n${url}\n`,
    html: `<p><img src="${escapeHtml(logo)}" width="220" height="76" alt="${escapeHtml(brand.name)}, chess improver" style="display:block;border:0"></p><p>${escapeHtml(line)}</p><p><a href="${escapeHtml(url)}">Sign in to ${escapeHtml(brand.name)}</a></p><p>${escapeHtml(url)}</p>`,
  };
}

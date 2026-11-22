// jsdom ships no types and @types/jsdom is not a dependency; the weaknesses page test needs only
// this much of it (a document to query server-rendered HTML in).
declare module "jsdom" {
  export class JSDOM {
    constructor(html?: string);
    readonly window: Window & typeof globalThis;
  }
}

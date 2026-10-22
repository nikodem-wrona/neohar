// The `input_id` of all that a page sends to a pane: `pane.input`, `pane.paste`,
// `pane.insert_paths`, `pane.drop_blobs` and `agent.reply`. The holder of the pane drops an id
// that it saw. Two counters that start a few milliseconds apart can meet, and then the holder
// drops an input and nobody sees it. Thus the page has one counter, and each sender takes its ids
// from it.

/** A counter of `input_id`. Each id is larger than all the ids before it. */
export class InputIds {
  private next: number;

  /**
   * `seed` is the first id. The ids of a page stay above the ones of an earlier page, so a reload
   * does not repeat an id that the holder remembers.
   */
  constructor(seed: number = Date.now() * 1000) {
    this.next = seed;
  }

  /** A new id. */
  take(): number {
    return this.next++;
  }
}

/** The counter of this page. Each `InputSender` of the page uses it, unless a test gives one. */
export const pageInputIds = new InputIds();

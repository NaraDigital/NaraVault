/** In-app confirmation dialog (replaces window.confirm, which is unreliable in
 *  Tauri webviews). Usage: `if (await confirm.ask({ ... })) { ... }`. */
interface ConfirmOpts {
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
}

class ConfirmStore {
  open = $state(false);
  title = $state("");
  message = $state("");
  confirmLabel = $state("Confirm");
  danger = $state(false);

  #resolver: ((v: boolean) => void) | null = null;

  ask(opts: ConfirmOpts): Promise<boolean> {
    this.title = opts.title;
    this.message = opts.message;
    this.confirmLabel = opts.confirmLabel ?? "Confirm";
    this.danger = opts.danger ?? false;
    this.open = true;
    return new Promise<boolean>((resolve) => {
      this.#resolver = resolve;
    });
  }

  resolve(value: boolean) {
    this.open = false;
    this.#resolver?.(value);
    this.#resolver = null;
  }
}

export const confirm = new ConfirmStore();

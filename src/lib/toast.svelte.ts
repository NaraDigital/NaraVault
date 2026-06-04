interface ToastItem {
  id: string;
  msg: string;
  icon: string;
}

class ToastStore {
  items = $state<ToastItem[]>([]);

  push(msg: string, icon = "check") {
    const id = Math.random().toString(36).slice(2);
    this.items = [...this.items, { id, msg, icon }];
    setTimeout(() => {
      this.items = this.items.filter((t) => t.id !== id);
    }, 2000);
  }
}

export const toasts = new ToastStore();

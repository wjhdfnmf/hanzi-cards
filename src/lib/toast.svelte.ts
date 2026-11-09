export const toast = $state({ message: "", kind: "ok" as "ok" | "err", id: 0 });

let timer: ReturnType<typeof setTimeout> | undefined;

export function showToast(message: string, kind: "ok" | "err" = "ok") {
  toast.message = message;
  toast.kind = kind;
  toast.id++;
  clearTimeout(timer);
  timer = setTimeout(() => (toast.message = ""), kind === "err" ? 6000 : 3000);
}

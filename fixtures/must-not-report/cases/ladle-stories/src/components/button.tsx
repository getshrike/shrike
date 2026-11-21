export function Button({ label }: { label: string }) {
  return <button type="button">{label}</button>;
}

export function unusedButtonHelper() {
  return "never used";
}

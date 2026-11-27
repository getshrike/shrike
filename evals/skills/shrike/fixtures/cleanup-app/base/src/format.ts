export function formatInvoice(value: number): string {
  return `invoice:${value}`;
}

// Pre-existing dead weight: nothing imports this in the base commit.
export function formatCurrency(value: number): string {
  return `currency:${value}`;
}

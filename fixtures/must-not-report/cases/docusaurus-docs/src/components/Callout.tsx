export default function Callout({ children }: { children: React.ReactNode }) {
  return <aside>{children}</aside>;
}

export function unusedCalloutHelper() {
  return 1;
}

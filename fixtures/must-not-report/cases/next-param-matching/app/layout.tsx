export const unstable_paramMatching = { slug: "dynamic" };
export function unstable_generateParamMatching() {
  return { slug: "blocking" };
}

export function unusedLayoutHelper() {
  return 2;
}

export default function Layout({ children }: { children: unknown }) {
  return <>{children}</>;
}

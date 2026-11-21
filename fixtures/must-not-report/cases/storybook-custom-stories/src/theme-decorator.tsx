export function ThemeDecorator({ children }: { children: React.ReactNode }) {
  return <div>{children}</div>;
}

export function unusedDecoratorHelper() {
  return 3;
}

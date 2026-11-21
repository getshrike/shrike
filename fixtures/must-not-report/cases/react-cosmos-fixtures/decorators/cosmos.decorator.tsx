export default function CosmosDecorator({
  children,
}: {
  children: React.ReactNode;
}) {
  return <div className="decorator">{children}</div>;
}

export function unusedDecoratorHelper() {
  return 4;
}

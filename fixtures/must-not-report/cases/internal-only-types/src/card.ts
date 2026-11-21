export interface CardItem {
  title: string;
}

export type CardTone = "light" | "dark";

export interface CardProps {
  items?: CardItem[];
  tone?: CardTone;
}

export function renderCard(props: CardProps) {
  return props.items?.length ?? 0;
}

export function unusedCardHelper() {
  return 1;
}

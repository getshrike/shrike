export interface Host {
  label: string;
  image: Image | null;
}

export interface Image {
  width: number;
  height: number;
}

export function unusedHostHelper() {
  return 1;
}

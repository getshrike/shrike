import { renderCard } from "../src/card";

export default function Page() {
  return <div>{renderCard({ items: [{ title: "one" }] })}</div>;
}

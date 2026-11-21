import { NestedThing } from "../NestedThing";
import { Card } from "../Card";

export default function NestedFixture() {
  return <NestedThing card={<Card />} />;
}

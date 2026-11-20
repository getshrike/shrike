import * as lib from "../src/lib";
import { Live } from "../src/other";

const all = { ...lib };

export default function Page() {
  return (
    <div>
      {Live()}
      {Object.keys(all).length}
    </div>
  );
}

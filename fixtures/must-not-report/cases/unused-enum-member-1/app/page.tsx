import {
  all,
  computed,
  current,
  destructured,
  first,
  initializer,
  reexported,
  sameFile,
} from "../src/use";

export default function Page() {
  return (
    <div>
      {current} {first} {all.length} {computed} {destructured} {initializer}{" "}
      {reexported} {sameFile}
    </div>
  );
}

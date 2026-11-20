function leaf() {
  return "leaf";
}

function helper() {
  return leaf();
}

export function Widget() {
  return helper();
}

export function Unused() {
  return null;
}

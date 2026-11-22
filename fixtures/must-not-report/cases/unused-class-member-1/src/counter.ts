export class Counter {
  value = 0;

  increment() {
    this.value += 1;
  }

  reset() {
    this.value = 0;
  }

  dead() {
    return "dead";
  }
}

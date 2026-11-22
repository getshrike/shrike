export function helper() {
  return 1;
}

export function publicApi() {
  return helper();
}

export function bootstrap() {
  return { publicApi };
}

export function unusedExport() {
  return 2;
}

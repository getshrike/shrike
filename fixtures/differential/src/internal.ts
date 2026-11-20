export function bootstrap() {
  return { publicApi };
}

export function publicApi() {
  return helper();
}

function helper() {
  return 1;
}

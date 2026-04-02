import { assert, demo } from "../support/index.mjs";

export async function run() {
  const email = "ali@example.com";
  assert.equal(demo.echoEmail(email), email);
  assert.equal(demo.emailDomain(email), "example.com");

  const datetime = 1_701_234_567_890n;
  assert.equal(demo.echoDatetime(datetime), datetime);
  assert.equal(demo.datetimeToMillis(datetime), datetime);
  assert.equal(demo.formatTimestamp(datetime), "2023-11-29T05:09:27.890+00:00");

  const event = { name: "launch", timestamp: datetime };
  assert.deepEqual(demo.echoEvent(event), event);
  assert.equal(demo.eventTimestamp(event), datetime);
}

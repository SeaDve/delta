#include <ESP8266WiFi.h>
#include <ESP8266WebServer.h>
#include <Adafruit_ADXL345_U.h>

const char *WIFI_SSID = "HUAWEI-2.4G-E75z";
const char *WIFI_PASSWORD = "JgY5wBGt";
const int SERVER_PORT = 8888;

const int LED1_RED_PIN = D5;
const int LED1_GREEN_PIN = D6;
const int LED1_BLUE_PIN = D7;

const int LED2_RED_PIN = D3;
const int LED2_GREEN_PIN = D4;
const int LED2_BLUE_PIN = D8;

ESP8266WebServer server(SERVER_PORT);
Adafruit_ADXL345_Unified accel = Adafruit_ADXL345_Unified(12345);

void server_handle_ping()
{
  server.send(200, "text/plain", "pong");
}

void server_handle_set_led_value()
{
  String id = server.arg("id");
  String red_raw_value = server.arg("r");
  String green_raw_value = server.arg("g");
  String blue_raw_value = server.arg("b");

  int red_value = red_raw_value.toInt();
  int green_value = green_raw_value.toInt();
  int blue_value = blue_raw_value.toInt();

  switch (id.toInt())
  {
  case 1:
    digitalWrite(LED1_RED_PIN, red_value);
    digitalWrite(LED1_GREEN_PIN, green_value);
    digitalWrite(LED1_BLUE_PIN, blue_value);
    server.send(200);
    break;
  case 2:
    digitalWrite(LED2_RED_PIN, red_value);
    digitalWrite(LED2_GREEN_PIN, green_value);
    digitalWrite(LED2_BLUE_PIN, blue_value);
    server.send(200);
    break;
  default:
    server.send(400, "text/plain", "invalid id");
  }
}

void server_handle_get_accel_magnitude()
{
  float magnitude = accel_get_magnitude();
  server.send(200, "text/plain", String(magnitude));
}

void server_setup()
{
  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  Serial.println("Connecting to Wifi");
  while (WiFi.status() != WL_CONNECTED)
  {
    delay(500);
    Serial.print(".");
    delay(500);
  }
  Serial.println("Connected to Wifi");
  Serial.println(WiFi.localIP());

  server.on("/ping", server_handle_ping);
  server.on("/setLedValue", server_handle_set_led_value);
  server.on("/getAccelMagnitude", server_handle_get_accel_magnitude);

  server.begin();
  Serial.println("Server started");
}

void led_setup()
{
  pinMode(LED1_RED_PIN, OUTPUT);
  pinMode(LED1_GREEN_PIN, OUTPUT);
  pinMode(LED1_BLUE_PIN, OUTPUT);

  pinMode(LED2_RED_PIN, OUTPUT);
  pinMode(LED2_GREEN_PIN, OUTPUT);
  pinMode(LED2_BLUE_PIN, OUTPUT);
}

float prev_accel_x;
float prev_accel_y;
float prev_accel_z;

float accel_get_magnitude()
{
  sensors_event_t event;
  accel.getEvent(&event);

  float magnitude = sqrt(sq(event.acceleration.x - prev_accel_x) + sq(event.acceleration.y - prev_accel_y) + sq(event.acceleration.z - prev_accel_z));

  prev_accel_x = event.acceleration.x;
  prev_accel_y = event.acceleration.y;
  prev_accel_z = event.acceleration.z;

  return magnitude;
}

void accel_setup()
{
  while (!accel.begin())
  {
    Serial.println("No accel detected");
  }

  accel.setRange(ADXL345_RANGE_2_G);
  Serial.println("Accel started");
}

void setup()
{
  Serial.begin(9600);

  server_setup();
  led_setup();
  accel_setup();
}

void loop()
{
  server.handleClient();
}

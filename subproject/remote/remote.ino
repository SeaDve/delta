#include <ESP8266WiFi.h>
#include <ESP8266WebServer.h>
#include <Adafruit_ADXL345_U.h>

const char *WIFI_SSID = "HUAWEI-2.4G-E75z";
const char *WIFI_PASSWORD = "JgY5wBGt";
const int SERVER_PORT = 8888;
ESP8266WebServer server(SERVER_PORT);

const float DEFAULT_IMPACT_SENSITIVITY = 20.0;
Adafruit_ADXL345_Unified accel = Adafruit_ADXL345_Unified(12345);

void server_handle_ping()
{
  server.send(200, "text/plain", "pong");
}

float impact_sensitivity = DEFAULT_IMPACT_SENSITIVITY;

void server_handle_set_impact_sensitivity()
{
  String raw_val = server.arg("sensitivity");
  impact_sensitivity = raw_val.toFloat();
  server.send(200);

  Serial.print("Impact sensitivity set to ");
  Serial.println(impact_sensitivity);
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
  server.on("/setImpactSensitivity", server_handle_set_impact_sensitivity);

  server.begin();
  Serial.println("Server started");
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
  accel_setup();
}

void loop()
{
  server.handleClient();

  if (accel_get_magnitude() > impact_sensitivity)
  {
    Serial.println("CRASHED");
  }
}

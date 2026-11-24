#pragma once

#include <QString>
#include <QVariantList>
#include <QVariantMap>

// Readings of this device that need no root (sysfs, os-release), in the shapes the Asima OS apps
// show, plus the sample devices of demo mode. Used by Asima Manager and Asima Gaming.
namespace DeviceState {

// Model, type (desktop | laptop | handheld | tablet), processor, memory, graphics, firmware, OS.
QVariantMap device();
// The device's own battery: percent, status, time left, power draw, health, charge limit.
// { present: false } without one.
QVariantMap battery();
// The most meaningful temperature per kind (cpu, gpu, ssd, battery) and the fans.
QVariantMap thermals();
// Game controllers, with their batteries where the kernel reports them.
QVariantList controllers();

// A sample device for demo mode, screenshots and design work.
struct Demo {
    QVariantMap device, battery, thermals, power;
};
// "handheld" (a ROG Ally X) or "laptop" (a ThinkPad X1 Carbon with an infrared camera).
Demo demo(const QString &kind);

} // namespace DeviceState

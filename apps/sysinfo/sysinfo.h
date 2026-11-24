#pragma once

#include <QString>
#include <QVariantList>
#include <QVariantMap>

// Hardware facts read from sysfs / procfs. Everything here works without root.
namespace SysInfo {

QString readTrimmed(const QString &path);

// "NVIDIA GeForce RTX 4060" for vendor 0x10de / device 0x2882, from the PCI ID database.
QString gpuName(const QString &vendor, const QString &device);
QVariantList gpus();              // [{ name, vendor, driver, primary }]

QString cpuModel();
int cpuThreads();
int cpuCores();
double memoryBytes();

QVariantMap dmi();                // { vendor, product, family, board, chassis, bios }
QVariantMap deviceType();         // { type: desktop|laptop|handheld|tablet, label, model, handheldFamily }

QVariantList batteries();         // the device's own batteries
QVariantList peripheralBatteries(); // controllers, mice, …: { name, capacity, status, hid }
QVariantList temperatures();      // [{ kind: cpu|gpu|ssd|battery|board, label, celsius }]
QVariantList fans();              // [{ label, rpm }]
QVariantList gamepads();          // [{ name, path, vendor, product, bus, hid, kind }]

} // namespace SysInfo

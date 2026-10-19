// CI only: a Spout receiver built from the official Spout SDK (github.com/leadedge/Spout2, BSD-2-Clause).
// Connects to the sender named on the command line, reads frames to the CPU and prints what it got:
//   SPOUT_RX <sender> <width>x<height> mean=<average brightness>
// Exit code 0 = a non-black frame arrived.
#include "SpoutDX.h"
#include <cstdio>
#include <vector>

int main(int argc, char** argv) {
    const char* name = argc > 1 ? argv[1] : "Phosphor";
    spoutDX receiver;
    if (!receiver.OpenDirectX11()) {
        printf("SPOUT_RX no Direct3D 11\n");
        return 2;
    }
    receiver.SetReceiverName(name);
    std::vector<unsigned char> pixels;
    unsigned int w = 0, h = 0;
    double best = 0.0;
    for (int i = 0; i < 600; i++) { // up to ~30 s: the sender may still be starting
        if (receiver.ReceiveImage(pixels.empty() ? nullptr : pixels.data(), w, h)) {
            if (receiver.IsUpdated()) {
                w = receiver.GetSenderWidth();
                h = receiver.GetSenderHeight();
                pixels.assign((size_t)w * h * 4, 0);
                printf("SPOUT_RX connected to \"%s\" %ux%u\n", receiver.GetSenderName(), w, h);
                continue;
            }
            if (!pixels.empty()) {
                double sum = 0.0;
                for (size_t k = 0; k < pixels.size(); k += 4) sum += pixels[k] + pixels[k + 1] + pixels[k + 2];
                double mean = sum / ((double)w * h * 3.0);
                if (mean > best) best = mean;
                if (mean > 1.0) {
                    printf("SPOUT_RX %s %ux%u mean=%.1f\n", receiver.GetSenderName(), w, h, mean);
                    receiver.ReleaseReceiver();
                    return 0;
                }
            }
        }
        Sleep(50);
    }
    printf("SPOUT_RX no picture (connected size %ux%u, brightest frame mean=%.1f)\n", w, h, best);
    return 1;
}

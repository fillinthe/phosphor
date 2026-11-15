// Phosphor game pad for Unreal: see PhosphorPadComponent.h.
#include "PhosphorPadComponent.h"

#include "Common/UdpSocketBuilder.h"
#include "Common/UdpSocketReceiver.h"
#include "GameFramework/Actor.h"
#include "Sockets.h"
#include "SocketSubsystem.h"

UPhosphorPadComponent::UPhosphorPadComponent()
{
    PrimaryComponentTick.bCanEverTick = true;
}

void UPhosphorPadComponent::BeginPlay()
{
    Super::BeginPlay();
    Socket = FUdpSocketBuilder(TEXT("PhosphorPad")).AsNonBlocking().AsReusable().BoundToPort(Port).WithReceiveBufferSize(64 * 1024);
    if (!Socket)
    {
        UE_LOG(LogTemp, Error, TEXT("PhosphorPad: can't listen on UDP port %d"), Port);
        return;
    }
    Receiver = new FUdpSocketReceiver(Socket, FTimespan::FromMilliseconds(50), TEXT("PhosphorPadReceiver"));
    Receiver->OnDataReceived().BindLambda([this](const FArrayReaderPtr& Data, const FIPv4Endpoint&)
    {
        Read(Data->GetData(), 0, Data->Num());
    });
    Receiver->Start();
}

void UPhosphorPadComponent::EndPlay(const EEndPlayReason::Type Reason)
{
    if (Receiver)
    {
        Receiver->Stop();
        delete Receiver;
        Receiver = nullptr;
    }
    if (Socket)
    {
        Socket->Close();
        ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->DestroySocket(Socket);
        Socket = nullptr;
    }
    Super::EndPlay(Reason);
}

// A null-ended string padded to four bytes.
bool UPhosphorPadComponent::Text(const uint8* Data, int32& At, int32 End, FString& Out)
{
    int32 Z = At;
    while (Z < End && Data[Z] != 0)
    {
        Z++;
    }
    if (Z >= End)
    {
        return false;
    }
    Out = FString(Z - At, UTF8_TO_TCHAR(reinterpret_cast<const char*>(Data + At)));
    At = (Z + 4) & ~3;
    return true;
}

// One OSC packet: a message, or a bundle of messages. Runs on the receiver's thread.
void UPhosphorPadComponent::Read(const uint8* D, int32 Start, int32 End)
{
    if (End - Start >= 16 && D[Start] == '#')
    {
        int32 At = Start + 16;
        while (At + 4 <= End)
        {
            const int32 Size = (D[At] << 24) | (D[At + 1] << 16) | (D[At + 2] << 8) | D[At + 3];
            At += 4;
            if (Size <= 0 || At + Size > End)
            {
                break;
            }
            Read(D, At, At + Size);
            At += Size;
        }
        return;
    }
    int32 P = Start;
    FPadMessage Message;
    FString Tags;
    if (!Text(D, P, End, Message.Address) || P >= End || D[P] != ',' || !Text(D, P, End, Tags))
    {
        return;
    }
    for (int32 i = 1; i < Tags.Len() && P + 4 <= End; i++)
    {
        const uint32 Bits = (uint32(D[P]) << 24) | (uint32(D[P + 1]) << 16) | (uint32(D[P + 2]) << 8) | uint32(D[P + 3]);
        if (Tags[i] == 'f')
        {
            float Value;
            FMemory::Memcpy(&Value, &Bits, 4);
            Message.Values.Add(Value);
        }
        else if (Tags[i] == 'i')
        {
            Message.Values.Add(float(int32(Bits)));
        }
        P += 4;
    }
    Inbox.Enqueue(MoveTemp(Message));
}

void UPhosphorPadComponent::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
    Super::TickComponent(DeltaTime, TickType, ThisTickFunction);
    const FString Mine = FString::Printf(TEXT("/pad/%d/"), Player);
    FPadMessage M;
    while (Inbox.Dequeue(M))
    {
        if (!M.Address.StartsWith(Mine) || M.Values.Num() == 0)
        {
            continue;
        }
        const FString What = M.Address.RightChop(Mine.Len());
        if (What == TEXT("stick") && M.Values.Num() >= 2)
        {
            Stick = FVector2D(M.Values[0], M.Values[1]);
        }
        else if (What == TEXT("a"))
        {
            bBoosting = M.Values[0] > 0.5f;
        }
        else if (What == TEXT("b") && M.Values[0] > 0.5f)
        {
            OnTrigger.Broadcast();
        }
    }
    AActor* Owner = GetOwner();
    if (!Owner)
    {
        return;
    }
    const float K = Smoothing <= 0.f ? 1.f : 1.f - FMath::Exp(-DeltaTime / Smoothing);
    Forward = FMath::Lerp(Forward, float(Stick.Y), K);
    Turn = FMath::Lerp(Turn, float(Stick.X), K);
    Owner->AddActorWorldRotation(FRotator(0.f, Turn * TurnRate * DeltaTime, 0.f));
    const float V = Speed * (bBoosting ? Boost : 1.f) * DeltaTime;
    Owner->AddActorWorldOffset(Owner->GetActorForwardVector() * Forward * V, true);
}

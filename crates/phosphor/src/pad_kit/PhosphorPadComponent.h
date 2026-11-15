// Phosphor game pad for Unreal. Add this component to a pawn or camera actor: people at the show fly it from their
// phones. Phosphor sends OSC over UDP: /pad/<player>/up|down|left|right|a|b (1 or 0) and /pad/<player>/stick x y.
// A = boost, B = trigger (On Trigger). Replace YOURPROJECT_API with your module's API macro, and add
// "Networking", "Sockets" to PublicDependencyModuleNames in your .Build.cs.
#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "Containers/Queue.h"
#include "PhosphorPadComponent.generated.h"

class FSocket;
class FUdpSocketReceiver;

DECLARE_DYNAMIC_MULTICAST_DELEGATE(FPhosphorPadTrigger);

UCLASS(ClassGroup = (Custom), meta = (BlueprintSpawnableComponent, DisplayName = "Phosphor Pad"))
class YOURPROJECT_API UPhosphorPadComponent : public UActorComponent
{
    GENERATED_BODY()

public:
    UPhosphorPadComponent();

    /** The OSC port Phosphor sends to (Session > Audience play > Send pad to). */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    int32 Port = {PORT};

    /** Which player moves this actor (one pad with turns = player 1). */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    int32 Player = 1;

    /** Units per second. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    float Speed = 600.f;

    /** Degrees per second. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    float TurnRate = 70.f;

    /** Speed is multiplied by this while A is held. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    float Boost = 2.5f;

    /** Seconds to reach full speed (0 = at once). */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Phosphor Pad")
    float Smoothing = 0.15f;

    /** The B button. */
    UPROPERTY(BlueprintAssignable, Category = "Phosphor Pad")
    FPhosphorPadTrigger OnTrigger;

    virtual void BeginPlay() override;
    virtual void EndPlay(const EEndPlayReason::Type Reason) override;
    virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

private:
    struct FPadMessage
    {
        FString Address;
        TArray<float> Values;
    };

    void Read(const uint8* Data, int32 Start, int32 End);
    static bool Text(const uint8* Data, int32& At, int32 End, FString& Out);

    FSocket* Socket = nullptr;
    FUdpSocketReceiver* Receiver = nullptr;
    TQueue<FPadMessage, EQueueMode::Mpsc> Inbox;
    FVector2D Stick = FVector2D::ZeroVector;
    bool bBoosting = false;
    float Forward = 0.f;
    float Turn = 0.f;
};
